//! Kennung des Spielstands der Quelltexte: Im Duell prüfen beide Browser, dass sie exakt dieselbe
//! Version spielen (sonst liefen die Simulationen auseinander).

use std::path::Path;

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, files);
        } else if p.extension().is_some_and(|x| x == "rs") {
            files.push(p);
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=src");
    let mut files = Vec::new();
    walk(Path::new("src"), &mut files);
    files.sort();
    // FNV-1a über Pfad und Inhalt
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in files {
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        eat(f.to_string_lossy().as_bytes());
        eat(&std::fs::read(&f).unwrap_or_default());
    }
    println!("cargo:rustc-env=FERRUM_BUILD_ID={h:016x}");
}
