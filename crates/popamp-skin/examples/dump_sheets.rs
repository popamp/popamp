//! `cargo run -p popamp-skin --example dump_sheets -- [SKIN] OUT_DIR`
//! Writes every sprite sheet of a skin (or the built-in one) as PNG.

use popamp_skin::{Sheet, Skin};
use std::path::PathBuf;

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let (skin, out) = match args.as_slice() {
        [out] => (Skin::builtin(), out.clone()),
        [skin, out] => (Skin::load(skin, None).expect("load skin"), out.clone()),
        _ => {
            eprintln!("usage: dump_sheets [SKIN] OUT_DIR");
            std::process::exit(2);
        }
    };
    std::fs::create_dir_all(&out).expect("create output dir");
    for s in Sheet::ALL.into_iter().filter(|&s| s != Sheet::NumsEx || skin.has_nums_ex()) {
        let b = skin.sheet(s);
        let path = out.join(s.file_name().replace(".bmp", ".png"));
        b.save_png(&path).expect("write png");
        println!("{:14} {}x{}", s.file_name(), b.w, b.h);
    }
}
