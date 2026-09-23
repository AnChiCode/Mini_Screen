// 在 Windows 上將 `assets/icon.ico` 嵌入為執行檔圖示；其他平台不做事。
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile()
            .expect("failed to embed the Windows icon resource");
    }
}
