//! Embeds the icon in the Windows executable, where Explorer and the taskbar
//! read it. Building for any other platform embeds nothing.

fn main() {
    println!("cargo:rerun-if-changed=assets/playr.rc");
    println!("cargo:rerun-if-changed=assets/playr.ico");
    embed_resource::compile("assets/playr.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("the icon resource compiles");
}
