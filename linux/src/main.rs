use std::env;
use std::path::PathBuf;
use typefix_core::{suggest, Dictionaries, Language, LayoutMap};

fn usage() {
    eprintln!("Usage: typefix-check <en|fa> <word> <source=target>...");
    eprintln!("Example: typefix-check en salam s=س a=ا l=ل m=م");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 3 { usage(); std::process::exit(2); }
    let Some(language) = Language::parse(&args[0]) else { usage(); std::process::exit(2) };

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Resources");
    let dicts = Dictionaries::load(root.join("en_50k.txt"), root.join("fa_50k.txt"))
        .and_then(|d| d.with_extra_english_words(root.join("words.txt")))
        .unwrap_or_else(|e| { eprintln!("Cannot load dictionaries: {e}"); std::process::exit(1) });

    let mut map = LayoutMap::default();
    for pair in &args[2..] {
        let Some((from, to)) = pair.split_once('=') else {
            eprintln!("Invalid mapping: {pair}"); std::process::exit(2);
        };
        let mut chars = from.chars();
        let Some(source) = chars.next() else { continue };
        if chars.next().is_some() {
            eprintln!("Mapping source must be one character: {pair}"); std::process::exit(2);
        }
        map.insert(source, to.to_owned());
    }

    match suggest(&dicts, &map, &args[1], language) {
        Some(s) => println!("{} -> {}", s.original, s.converted),
        None => println!("No suggestion"),
    }
}
