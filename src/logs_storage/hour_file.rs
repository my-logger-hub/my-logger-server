use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::repo::DateHourKey;

use super::{
    compress_records, decompress_records, Cell, DataSource, Frame, HourCells, HourKeyIndex,
    KeyIndex, KeyIndexDirectory, LogRecordProto,
    FRAME_CAPACITY, FRAME_HEADER_LEN, FRAME_LEN, INDEX_DIRTY, INDEX_STATE_OFFSET,
    SECONDS_PER_HOUR,
};

pub const MICROS_PER_SECOND: i64 = 1_000_000;

/// Offsets are `u32`: a data file holds at most this many frames - 4 GiB
const MAX_FRAMES: u32 = ((u32::MAX as u64 + 1) / FRAME_LEN as u64) as u32;

const CHAIN_LOOP: &str = "The chain of frames returns to a frame it walked";

/// The key index is written at most this often while the hour is written into: it is kept
/// in memory, and one which did not get its last changes is rebuilt from the data file
const KEY_INDEX_WRITE_INTERVAL: Duration = Duration::from_secs(5);

pub fn data_file_name(hour: DateHourKey) -> String {
    format!("{}.data", hour.get_value())
}

pub fn index_file_name(hour: DateHourKey) -> String {
    format!("{}.index", hour.get_value())
}

pub fn key_index_file_name(hour: DateHourKey) -> String {
    format!("{}.index-by-key", hour.get_value())
}

/// The second of the hour starting at `hour_start` the time falls into
pub fn second_of(hour_start: i64, time: i64) -> Option<usize> {
    let second = (time - hour_start).div_euclid(MICROS_PER_SECOND);

    if (0..SECONDS_PER_HOUR as i64).contains(&second) {
        Some(second as usize)
    } else {
        None
    }
}

/// Where an entry is, or goes: its frame and its offset from the frame's start
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    frame: u32,
    offset: usize,
}

impl Position {
    fn of(cell: Cell) -> Self {
        Self {
            frame: cell.frame,
            offset: cell.offset as usize,
        }
    }

    fn to_cell(self) -> Cell {
        Cell {
            frame: self.frame,
            offset: self.offset as u32,
        }
    }
}

/// The files of one hour as their writer keeps them: the cells in memory, the frames an
/// insert has read or changed, and the changes until they are flushed.
pub struct HourFile {
    hour_start: i64,
    data_path: PathBuf,
    index_path: PathBuf,
    /// The data file and the index, once they exist
    files: Option<(File, File)>,
    cells: HourCells,
    /// Frames of the data file, flushed or not
    frames_count: u32,
    /// Frames the data file has on disk
    frames_on_disk: u32,
    /// The last frame of the chain
    tail: Option<u32>,
    last_second: Option<usize>,
    frames: HashMap<u32, Frame>,
    /// Frames changed since the last flush, in the order they are to be written
    dirty: Vec<u32>,
    index_clean_on_disk: bool,
    /// Uploaded, or being uploaded: nothing is written into it any more
    sealed: bool,
    key_index_path: PathBuf,
    key_index: KeyIndexState,
    key_index_changed: bool,
    key_index_written: Option<Instant>,
    indexed_keys: Vec<String>,
}

enum KeyIndexState {
    /// The hour is written into: the whole index, kept up to date by the writes
    Full(Arc<KeyIndex>),
    /// The hour is read: only the page of its keys and values
    Directory(Arc<KeyIndexDirectory>),
}

impl HourFile {
    /// The files of the hour in `dir`. Nothing is created until the first flush. An index
    /// which is not clean - its flush did not complete - is rebuilt from the data file, and
    /// so is a key index which does not count the records the index does. A new hour indexes
    /// `indexed_keys`. The key index of an hour to be written - `full_key_index` - is read
    /// whole, of any other only its page of keys and values.
    pub fn open(
        dir: &Path,
        hour: DateHourKey,
        indexed_keys: &[String],
        full_key_index: bool,
    ) -> std::io::Result<Self> {
        let mut result = Self {
            hour_start: hour.hour_start().unix_microseconds,
            data_path: dir.join(data_file_name(hour)),
            index_path: dir.join(index_file_name(hour)),
            files: None,
            cells: HourCells::new(),
            frames_count: 0,
            frames_on_disk: 0,
            tail: None,
            last_second: None,
            frames: HashMap::new(),
            dirty: Vec::new(),
            index_clean_on_disk: false,
            sealed: false,
            key_index_path: dir.join(key_index_file_name(hour)),
            key_index: KeyIndexState::Full(Arc::new(KeyIndex::new(indexed_keys))),
            key_index_changed: false,
            key_index_written: None,
            indexed_keys: indexed_keys.to_vec(),
        };

        if !result.data_path.exists() {
            return Ok(result);
        }

        let data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&result.data_path)?;
        let index_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&result.index_path)?;

        let data_len = data_file.metadata()?.len();
        result.frames_count = (data_len / FRAME_LEN as u64) as u32;
        result.frames_on_disk = result.frames_count;

        let mut index_bytes = Vec::new();
        (&index_file).read_to_end(&mut index_bytes)?;

        result.files = Some((data_file, index_file));

        match HourCells::from_index_file(&index_bytes) {
            Some(cells) if data_len % FRAME_LEN as u64 == 0 => {
                result.cells = cells;
                result.index_clean_on_disk = true;
            }
            _ => {
                println!(
                    "The index of {} is not clean: it is rebuilt from the data file",
                    result.data_path.display()
                );
                result.rebuild(data_len)?;
            }
        }

        result.last_second = result.cells.last().map(|(second, _)| second);
        result.tail = result.find_tail()?;

        let total = result.cells.total();

        let key_index = if full_key_index {
            result
                .read_full_key_index()
                .map(|index| KeyIndexState::Full(Arc::new(index)))
        } else {
            KeyIndexDirectory::read_local(&result.key_index_path)
                .filter(|directory| directory.total_records() == total)
                .map(|directory| KeyIndexState::Directory(Arc::new(directory)))
        };

        match key_index {
            Some(key_index) => result.key_index = key_index,
            None => {
                println!(
                    "The key index of {} is rebuilt from the data file",
                    result.data_path.display()
                );
                result.key_index = KeyIndexState::Full(Arc::new(result.rebuild_key_index()?));
                result.key_index_changed = true;
                result.write_key_index()?;
            }
        }

        Ok(result)
    }

    /// The key index file whole, when it counts the records the index does
    fn read_full_key_index(&self) -> Option<KeyIndex> {
        std::fs::read(&self.key_index_path)
            .ok()
            .and_then(|bytes| KeyIndex::from_file(&bytes))
            .filter(|index| index.total_records() == self.cells.total())
    }

    /// The key index as a search uses it
    pub fn key_index(&self) -> HourKeyIndex {
        match &self.key_index {
            KeyIndexState::Full(index) => HourKeyIndex::Full(index.clone()),
            KeyIndexState::Directory(directory) => HourKeyIndex::Directory {
                directory: directory.clone(),
                source: DataSource::Local(self.key_index_path.clone()),
            },
        }
    }

    /// The whole key index, to be written into: read when only its page was
    fn full_key_index(&mut self) -> Result<&mut KeyIndex, String> {
        if let KeyIndexState::Directory(_) = self.key_index {
            let index = match self.read_full_key_index() {
                Some(index) => index,
                None => self.rebuild_key_index().map_err(|err| err.to_string())?,
            };

            self.key_index = KeyIndexState::Full(Arc::new(index));
        }

        match &mut self.key_index {
            KeyIndexState::Full(index) => Ok(Arc::make_mut(index)),
            KeyIndexState::Directory(_) => unreachable!(),
        }
    }

    /// The whole key index, as the files have it
    #[cfg(test)]
    pub fn whole_key_index(&self) -> KeyIndex {
        match &self.key_index {
            KeyIndexState::Full(index) => index.as_ref().clone(),
            KeyIndexState::Directory(_) => self.read_full_key_index().unwrap(),
        }
    }

    pub fn hour_start(&self) -> i64 {
        self.hour_start
    }

    pub fn cells(&self) -> &HourCells {
        &self.cells
    }

    pub fn data_path(&self) -> &Path {
        &self.data_path
    }

    pub fn has_entries(&self) -> bool {
        self.tail.is_some()
    }

    pub fn cached_frames(&self) -> usize {
        self.frames.len()
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn set_sealed(&mut self, sealed: bool) {
        self.sealed = sealed;
    }

    /// Writes the records of one of its seconds, in time order. A second which has entries
    /// already gets them merged into its last block: the block is decompressed, the
    /// records are added and it is compressed again - into two blocks when it outgrows
    /// one - and it takes the place of the old one.
    pub fn write_second(
        &mut self,
        second: usize,
        mut records: Vec<LogRecordProto>,
    ) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }

        self.full_key_index()?.add_records(second, &records);
        self.key_index_changed = true;

        let mut to_remove = None;
        // The records of the second which are in the block taken out
        let mut removed_records = 0;

        let (mut place, first_of_second) = match self.last_entry_of(second)? {
            Some((last, len)) => match self.decompress_at(last) {
                Ok(mut merged) => {
                    removed_records = merged.len();
                    merged.append(&mut records);
                    merged.sort_by_key(|record| record.timestamp);
                    records = merged;

                    to_remove = Some((last, len));
                    (Some(last), self.cells.get(second) == last.to_cell())
                }
                Err(err) => {
                    println!(
                        "The last block of second {} of {} does not read: {}. The records go into a block of their own",
                        second,
                        self.data_path.display(),
                        err
                    );

                    let after = Position {
                        frame: last.frame,
                        offset: last.offset + len,
                    };
                    (Some(after), false)
                }
            },
            None => (self.place_for_new_second(second)?, true),
        };

        let blocks = compress_records(records);

        // Each entry takes at most two frames more: decided before anything changes
        if self.frames_count as usize + 2 * blocks.len() > MAX_FRAMES as usize {
            return Err(format!(
                "The data file {} has {} frames, and there is no room for more",
                self.data_path.display(),
                self.frames_count
            ));
        }

        if let Some((position, len)) = to_remove {
            self.remove_entry(position, len);
        }

        for (index, block) in blocks.iter().enumerate() {
            let cell = self.insert_at(place, &block.entry)?;

            if index == 0 && first_of_second {
                self.cells.set(second, cell);
            }

            place = Some(Position {
                frame: cell.frame,
                offset: cell.offset as usize + block.entry.len(),
            });
        }

        let written: usize = blocks.iter().map(|block| block.records).sum();
        let count = self.cells.count(second) as usize - removed_records + written;
        self.cells.set_count(second, count as u32);

        self.last_second = Some(self.last_second.map_or(second, |last| last.max(second)));

        Ok(())
    }

    /// Writes every changed frame, then the cells; the key index when it is changed and
    /// it is time - or when `force_key_index`.
    pub fn flush(&mut self, force_key_index: bool) -> std::io::Result<()> {
        if !self.dirty.is_empty() {
            self.flush_frames()?;
        }

        let due = self
            .key_index_written
            .map_or(true, |written| written.elapsed() >= KEY_INDEX_WRITE_INTERVAL);

        if self.key_index_changed && (due || force_key_index) {
            self.write_key_index()?;
        }

        Ok(())
    }

    /// Next to the file and renamed over it: a key index is whole, or it is the one before
    fn write_key_index(&mut self) -> std::io::Result<()> {
        let KeyIndexState::Full(index) = &self.key_index else {
            // Only its page is read: nothing is changed
            return Ok(());
        };

        let mut temp = self.key_index_path.clone().into_os_string();
        temp.push(".tmp");

        std::fs::write(&temp, index.to_file())?;
        std::fs::rename(&temp, &self.key_index_path)?;

        self.key_index_changed = false;
        self.key_index_written = Some(Instant::now());

        Ok(())
    }

    /// The index is marked dirty before the first frame goes out, so a flush which does
    /// not complete leaves an index that is rebuilt on the next open.
    fn flush_frames(&mut self) -> std::io::Result<()> {
        if self.files.is_none() {
            if let Some(dir) = self.data_path.parent() {
                std::fs::create_dir_all(dir)?;
            }

            let data_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&self.data_path)?;
            let index_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&self.index_path)?;

            self.files = Some((data_file, index_file));
        }

        let (data_file, index_file) = self.files.as_ref().unwrap();

        if self.index_clean_on_disk {
            index_file.write_all_at(&[INDEX_DIRTY], INDEX_STATE_OFFSET)?;
            self.index_clean_on_disk = false;
        }

        // The new frames first: nothing points at them until the changed ones are written
        let frames_on_disk = self.frames_on_disk;
        let (mut new_frames, changed_frames): (Vec<u32>, Vec<u32>) = self
            .dirty
            .iter()
            .partition(|offset| **offset / FRAME_LEN as u32 >= frames_on_disk);
        new_frames.sort();

        for offset in new_frames.iter().chain(changed_frames.iter()) {
            data_file.write_all_at(self.frames[offset].as_bytes(), *offset as u64)?;
        }

        index_file.write_all_at(&self.cells.to_index_file(), 0)?;
        self.index_clean_on_disk = true;

        self.frames_on_disk = self.frames_count;
        self.dirty.clear();

        // The last frame is kept: it is where the next seconds go
        let tail = self.tail;
        self.frames.retain(|offset, _| Some(*offset) == tail);

        Ok(())
    }

    /// Writes the chain into new files frame after frame, each one filled before the next
    /// one starts: what is uploaded is read as one run. Expects the hour to be flushed.
    pub fn compact_to(
        &self,
        data_path: &Path,
        index_path: &Path,
        key_index_path: &Path,
    ) -> std::io::Result<()> {
        let mut data = std::io::BufWriter::new(File::create(data_path)?);
        let mut cells = HourCells::new();

        if self.tail.is_some() {
            let mut out = Frame::new();
            let mut out_offset: u32 = 0;
            let mut frame_offset = 0;
            let mut hops = 0;

            loop {
                let frame = self.read_frame(frame_offset)?;
                let mut offset = FRAME_HEADER_LEN;

                while offset < frame.records_end() {
                    let entry = frame.entry_at(offset).map_err(corrupted)?;

                    if out.free() < entry.len {
                        out.set_next(out_offset + FRAME_LEN as u32);
                        data.write_all(out.as_bytes())?;
                        out_offset += FRAME_LEN as u32;
                        out = Frame::new();
                    }

                    let at = out.records_end();
                    out.insert(at, &frame.as_bytes()[offset..offset + entry.len]);

                    if let Some(second) = second_of(self.hour_start, entry.time) {
                        if cells.get(second).is_empty() {
                            cells.set(
                                second,
                                Cell {
                                    frame: out_offset,
                                    offset: at as u32,
                                },
                            );
                        }
                    }

                    offset += entry.len;
                }

                if frame.next() == 0 {
                    break;
                }

                hops += 1;
                if hops > self.frames_count {
                    return Err(corrupted(CHAIN_LOOP));
                }

                frame_offset = frame.next();
            }

            if out.used() > 0 {
                data.write_all(out.as_bytes())?;
            }
        }

        for second in 0..SECONDS_PER_HOUR {
            cells.set_count(second, self.cells.count(second));
        }

        data.flush()?;
        std::fs::write(index_path, cells.to_index_file())?;
        match &self.key_index {
            KeyIndexState::Full(index) => std::fs::write(key_index_path, index.to_file())?,
            KeyIndexState::Directory(_) => {
                std::fs::copy(&self.key_index_path, key_index_path)?;
            }
        }

        Ok(())
    }

    /// The last entry of the second and its length
    fn last_entry_of(&mut self, second: usize) -> Result<Option<(Position, usize)>, String> {
        let cell = self.cells.get(second);

        if cell.is_empty() {
            return Ok(None);
        }

        let second_end = self.hour_start + (second as i64 + 1) * MICROS_PER_SECOND;
        let mut position = Position::of(cell);
        let mut last = None;

        for _ in 0..=self.frames_count {
            let frame = self.frame(position.frame)?;

            while position.offset < frame.records_end() {
                let entry = frame.entry_at(position.offset)?;

                if entry.time >= second_end {
                    return Ok(last);
                }

                last = Some((position, entry.len));
                position.offset += entry.len;
            }

            if frame.next() == 0 {
                return Ok(last);
            }

            position = Position {
                frame: frame.next(),
                offset: FRAME_HEADER_LEN,
            };
        }

        Err(CHAIN_LOOP.to_string())
    }

    fn decompress_at(&mut self, position: Position) -> Result<Vec<LogRecordProto>, String> {
        let frame = self.frame(position.frame)?;
        let entry = frame.entry_at(position.offset)?;
        decompress_records(entry.payload)
    }

    /// A second with no entries goes before the first entry of the nearest later second
    /// which has one, at the end of the chain otherwise. `None` - the file has no entries.
    fn place_for_new_second(&mut self, second: usize) -> Result<Option<Position>, String> {
        let later = match self.last_second {
            Some(last) if last > second => self.cells.first_from(second + 1),
            _ => None,
        };

        if let Some((_, cell)) = later {
            return Ok(Some(Position::of(cell)));
        }

        match self.tail {
            Some(tail) => {
                let offset = self.frame(tail)?.records_end();
                Ok(Some(Position {
                    frame: tail,
                    offset,
                }))
            }
            None => Ok(None),
        }
    }

    /// Puts the entry at the place: into its frame while it has room, the entries after
    /// the place moving up; into a new frame at the end of the chain; into one half of the
    /// frame, halved, when it is full.
    fn insert_at(&mut self, place: Option<Position>, entry: &[u8]) -> Result<Cell, String> {
        let Some(mut place) = place else {
            let frame = self.allocate_frame()?;
            self.frames
                .get_mut(&frame)
                .unwrap()
                .insert(FRAME_HEADER_LEN, entry);
            self.mark_dirty(frame);
            self.tail = Some(frame);

            return Ok(Cell {
                frame,
                offset: FRAME_HEADER_LEN as u32,
            });
        };

        for _ in 0..=self.frames_count {
            let (free, records_end, next) = {
                let frame = self.frame(place.frame)?;
                (frame.free(), frame.records_end(), frame.next())
            };

            if entry.len() <= free {
                return Ok(self.insert_into_frame(place, entry));
            }

            if place.offset < records_end {
                return self.halve_and_insert(place, entry);
            }

            if next == 0 {
                return self.append_frame(place.frame, entry);
            }

            // The end of a full frame is the start of the next one
            place = Position {
                frame: next,
                offset: FRAME_HEADER_LEN,
            };
        }

        Err(CHAIN_LOOP.to_string())
    }

    fn insert_into_frame(&mut self, place: Position, entry: &[u8]) -> Cell {
        self.frames
            .get_mut(&place.frame)
            .unwrap()
            .insert(place.offset, entry);

        for cell in self.cells.iter_mut() {
            if cell.frame == place.frame && cell.offset as usize >= place.offset {
                cell.offset += entry.len() as u32;
            }
        }

        self.mark_dirty(place.frame);
        place.to_cell()
    }

    fn append_frame(&mut self, last: u32, entry: &[u8]) -> Result<Cell, String> {
        let frame = self.allocate_frame()?;

        self.frames
            .get_mut(&frame)
            .unwrap()
            .insert(FRAME_HEADER_LEN, entry);
        self.frames.get_mut(&last).unwrap().set_next(frame);

        self.mark_dirty(frame);
        self.mark_dirty(last);
        self.tail = Some(frame);

        Ok(Cell {
            frame,
            offset: FRAME_HEADER_LEN as u32,
        })
    }

    /// The frame is full and the place is before one of its entries. The frame is halved at
    /// the entry boundary nearest the middle of its entries, ties to the lower one; the
    /// entry goes into the lower half when its place is at or before the boundary. The
    /// upper half moves in front of the entries of the next frame when that one has room
    /// for it, into a new frame chained between the two otherwise.
    fn halve_and_insert(&mut self, place: Position, entry: &[u8]) -> Result<Cell, String> {
        let next = self.frames[&place.frame].next();

        // Everything the insert touches is read before anything changes
        if next != 0 {
            self.frame(next)?;
        }

        let (boundary, records_end) = {
            let frame = &self.frames[&place.frame];
            let middle = FRAME_HEADER_LEN + frame.used() / 2;

            let boundary = frame
                .entry_offsets()?
                .into_iter()
                .skip(1)
                .min_by_key(|boundary| (boundary.abs_diff(middle), *boundary))
                .ok_or_else(|| format!("The full frame at {} has a single entry", place.frame))?;

            (boundary, frame.records_end())
        };

        let into_lower = place.offset <= boundary;
        let lower_len = boundary - FRAME_HEADER_LEN + if into_lower { entry.len() } else { 0 };
        let upper_len = records_end - boundary + if into_lower { 0 } else { entry.len() };

        if lower_len > FRAME_CAPACITY || upper_len > FRAME_CAPACITY {
            return Err(format!(
                "An entry of {} bytes fits neither half of the frame at {}",
                entry.len(),
                place.frame
            ));
        }

        let into_next = next != 0 && self.frames[&next].free() >= upper_len;

        if !into_next && self.frames_count >= MAX_FRAMES {
            return Err(format!(
                "The data file {} has {} frames, and there is no room for more",
                self.data_path.display(),
                self.frames_count
            ));
        }

        // Nothing fails from here on
        let moved: Vec<usize> = (0..SECONDS_PER_HOUR)
            .filter(|second| {
                let cell = self.cells.get(*second);
                !cell.is_empty() && cell.frame == place.frame && cell.offset as usize >= boundary
            })
            .collect();

        let mut upper = self
            .frames
            .get_mut(&place.frame)
            .unwrap()
            .split_off(boundary);

        let mut entry_cell = None;

        if into_lower {
            self.frames
                .get_mut(&place.frame)
                .unwrap()
                .insert(place.offset, entry);

            for cell in self.cells.iter_mut() {
                let offset = cell.offset as usize;
                if cell.frame == place.frame && offset >= place.offset && offset < boundary {
                    cell.offset += entry.len() as u32;
                }
            }

            entry_cell = Some(place.to_cell());
        } else {
            let after = upper.split_off(place.offset - boundary);
            upper.extend_from_slice(entry);
            upper.extend_from_slice(&after);
        }

        let destination = if into_next {
            for cell in self.cells.iter_mut() {
                if cell.frame == next {
                    cell.offset += upper.len() as u32;
                }
            }

            self.frames
                .get_mut(&next)
                .unwrap()
                .insert(FRAME_HEADER_LEN, &upper);
            self.mark_dirty(next);
            next
        } else {
            let frame = self.allocate_frame()?;

            let new_frame = self.frames.get_mut(&frame).unwrap();
            new_frame.insert(FRAME_HEADER_LEN, &upper);
            new_frame.set_next(next);

            self.frames.get_mut(&place.frame).unwrap().set_next(frame);

            if next == 0 {
                self.tail = Some(frame);
            }

            self.mark_dirty(frame);
            frame
        };

        self.mark_dirty(place.frame);

        for second in moved {
            let mut at = self.cells.get(second).offset as usize - boundary;

            if !into_lower && at >= place.offset - boundary {
                at += entry.len();
            }

            self.cells.set(
                second,
                Cell {
                    frame: destination,
                    offset: (FRAME_HEADER_LEN + at) as u32,
                },
            );
        }

        Ok(entry_cell.unwrap_or_else(|| Cell {
            frame: destination,
            offset: (FRAME_HEADER_LEN + place.offset - boundary) as u32,
        }))
    }

    /// Takes the entry out of its frame: the entries after it move down. The frame is in
    /// memory - the entry was found in it.
    fn remove_entry(&mut self, position: Position, len: usize) {
        self.frames
            .get_mut(&position.frame)
            .unwrap()
            .remove(position.offset, len);

        for cell in self.cells.iter_mut() {
            if cell.frame == position.frame && cell.offset as usize > position.offset {
                cell.offset -= len as u32;
            }
        }

        self.mark_dirty(position.frame);
    }

    fn allocate_frame(&mut self) -> Result<u32, String> {
        if self.frames_count >= MAX_FRAMES {
            return Err(format!(
                "The data file {} has {} frames, and there is no room for more",
                self.data_path.display(),
                self.frames_count
            ));
        }

        let offset = self.frames_count * FRAME_LEN as u32;
        self.frames_count += 1;
        self.frames.insert(offset, Frame::new());

        Ok(offset)
    }

    fn mark_dirty(&mut self, frame: u32) {
        if !self.dirty.contains(&frame) {
            self.dirty.push(frame);
        }
    }

    /// The frame, read into memory for writing
    fn frame(&mut self, offset: u32) -> Result<&mut Frame, String> {
        if !self.frames.contains_key(&offset) {
            let frame = self
                .read_frame_from_disk(offset)
                .map_err(|err| err.to_string())?;
            self.frames.insert(offset, frame);
        }

        Ok(self.frames.get_mut(&offset).unwrap())
    }

    /// The frame, without keeping it in memory
    fn read_frame(&self, offset: u32) -> std::io::Result<Frame> {
        match self.frames.get(&offset) {
            Some(frame) => Ok(frame.clone()),
            None => self.read_frame_from_disk(offset),
        }
    }

    fn read_frame_from_disk(&self, offset: u32) -> std::io::Result<Frame> {
        if offset % FRAME_LEN as u32 != 0 || offset / FRAME_LEN as u32 >= self.frames_on_disk {
            return Err(corrupted(format!(
                "{} has no frame at {}",
                self.data_path.display(),
                offset
            )));
        }

        let (data_file, _) = self.files.as_ref().unwrap();

        let mut bytes = vec![0u8; FRAME_LEN];
        data_file.read_exact_at(&mut bytes, offset as u64)?;

        Frame::from_bytes(bytes).map_err(corrupted)
    }

    fn find_tail(&self) -> std::io::Result<Option<u32>> {
        if self.frames_count == 0 {
            return Ok(None);
        }

        let mut offset = self.cells.last().map(|(_, cell)| cell.frame).unwrap_or(0);

        for _ in 0..=self.frames_count {
            let next = self.read_frame(offset)?.next();

            if next == 0 {
                return Ok(Some(offset));
            }

            offset = next;
        }

        Err(corrupted(CHAIN_LOOP))
    }

    /// Counts the keys of every record of the chain again
    fn rebuild_key_index(&self) -> std::io::Result<KeyIndex> {
        let mut result = KeyIndex::new(&self.indexed_keys);

        if self.tail.is_none() {
            return Ok(result);
        }

        let mut frame_offset = 0;

        for _ in 0..=self.frames_count {
            let frame = self.read_frame(frame_offset)?;
            let mut offset = FRAME_HEADER_LEN;

            while offset < frame.records_end() {
                let entry = frame.entry_at(offset).map_err(corrupted)?;

                if let (Some(second), Ok(records)) = (
                    second_of(self.hour_start, entry.time),
                    decompress_records(entry.payload),
                ) {
                    result.add_records(second, &records);
                }

                offset += entry.len;
            }

            if frame.next() == 0 {
                return Ok(result);
            }

            frame_offset = frame.next();
        }

        Err(corrupted(CHAIN_LOOP))
    }

    /// Finds the cells again by walking the chain from its head. A frame, a link or an
    /// entry which does not read ends the chain there: the frame keeps its entries before
    /// it and becomes the last one.
    fn rebuild(&mut self, data_len: u64) -> std::io::Result<()> {
        self.cells = HourCells::new();

        let (data_file, index_file) = self.files.as_ref().unwrap();

        if data_len % FRAME_LEN as u64 != 0 {
            data_file.set_len(self.frames_count as u64 * FRAME_LEN as u64)?;
        }

        let mut visited = HashSet::new();
        let mut frame_offset = 0;

        while self.frames_count > 0 {
            visited.insert(frame_offset);

            let mut frame = match self.read_frame_from_disk(frame_offset) {
                Ok(frame) => frame,
                Err(_) => {
                    // Not even the head reads: there is nothing in the file
                    data_file.set_len(0)?;
                    self.frames_count = 0;
                    self.frames_on_disk = 0;
                    break;
                }
            };

            let mut offset = FRAME_HEADER_LEN;
            let mut cut = false;

            while offset < frame.records_end() {
                let second = frame
                    .entry_at(offset)
                    .ok()
                    .and_then(|entry| second_of(self.hour_start, entry.time).map(|s| (s, entry.len)));

                match second {
                    Some((second, len)) => {
                        if self.cells.get(second).is_empty() {
                            self.cells.set(
                                second,
                                Cell {
                                    frame: frame_offset,
                                    offset: offset as u32,
                                },
                            );
                        }

                        // The number of records is in the block: it is decompressed
                        let records = frame
                            .entry_at(offset)
                            .ok()
                            .and_then(|entry| decompress_records(entry.payload).ok())
                            .map_or(0, |records| records.len() as u32);
                        self.cells
                            .set_count(second, self.cells.count(second) + records);

                        offset += len;
                    }
                    None => {
                        cut = true;
                        break;
                    }
                }
            }

            let next = frame.next();
            let next_reads = next != 0
                && !visited.contains(&next)
                && self.read_frame_from_disk(next).is_ok();

            if cut || (next != 0 && !next_reads) {
                if cut {
                    frame.split_off(offset);
                }

                frame.set_next(0);
                data_file.write_all_at(frame.as_bytes(), frame_offset as u64)?;
                break;
            }

            if next == 0 {
                break;
            }

            frame_offset = next;
        }

        index_file.write_all_at(&self.cells.to_index_file(), 0)?;
        index_file.set_len(super::INDEX_FILE_LEN as u64)?;
        self.index_clean_on_disk = true;

        Ok(())
    }

    /// Every entry of the chain in its order: its frame, offset and time
    #[cfg(test)]
    pub fn walk(&self) -> Vec<(u32, usize, i64)> {
        let mut result = Vec::new();

        if self.tail.is_none() {
            return result;
        }

        let mut frame_offset = 0;

        loop {
            let frame = self.read_frame(frame_offset).unwrap();
            let mut offset = FRAME_HEADER_LEN;

            while offset < frame.records_end() {
                let entry = frame.entry_at(offset).unwrap();
                result.push((frame_offset, offset, entry.time));
                offset += entry.len;
            }

            if frame.next() == 0 {
                return result;
            }

            frame_offset = frame.next();
        }
    }

    /// The records of every entry of the chain in its order
    #[cfg(test)]
    pub fn read_all(&self) -> Vec<LogRecordProto> {
        self.walk()
            .into_iter()
            .flat_map(|(frame_offset, offset, _)| {
                let frame = self.read_frame(frame_offset).unwrap();
                let entry = frame.entry_at(offset).unwrap();
                decompress_records(entry.payload).unwrap()
            })
            .collect()
    }
}

fn corrupted(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}
