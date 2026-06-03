fn main() {
    // Only compile resources on Windows target
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        embed_resource::compile("stratum.rc", embed_resource::NONE);
    }
}
