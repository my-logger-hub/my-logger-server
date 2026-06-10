pub struct JemallocStats {
    pub allocated: usize,
    pub active: usize,
    pub resident: usize,
    pub mapped: usize,
    pub retained: usize,
    pub metadata: usize,
}

impl JemallocStats {
    pub fn to_debug_string(&self) -> String {
        format!(
            "jemalloc: allocated={}MB active={}MB resident={}MB mapped={}MB retained={}MB metadata={}MB",
            self.allocated / 1024 / 1024,
            self.active / 1024 / 1024,
            self.resident / 1024 / 1024,
            self.mapped / 1024 / 1024,
            self.retained / 1024 / 1024,
            self.metadata / 1024 / 1024
        )
    }
}

pub fn read_jemalloc_stats() -> Option<JemallocStats> {
    use tikv_jemalloc_ctl::{epoch, stats};

    epoch::advance().ok()?;

    Some(JemallocStats {
        allocated: stats::allocated::read().ok()?,
        active: stats::active::read().ok()?,
        resident: stats::resident::read().ok()?,
        mapped: stats::mapped::read().ok()?,
        retained: stats::retained::read().ok()?,
        metadata: stats::metadata::read().ok()?,
    })
}
