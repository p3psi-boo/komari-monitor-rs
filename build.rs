fn main() {
    #[cfg(all(feature = "winxp-support", target_os = "windows"))]
    thunk::thunk();
}
