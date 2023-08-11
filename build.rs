fn main() {
    let url = "https://raw.githubusercontent.com/my-logger-hub/proto-files/main/";
    ci_utils::sync_and_build_proto_file(url, "MyLogger.proto");
}
