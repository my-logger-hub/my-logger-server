fn main() {
    ci_utils::ProtoFileBuilder::new("proto/").sync_and_build("MyLogger.proto");
}
