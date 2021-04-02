use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use quote::quote;

fn main() -> io::Result<()> {
    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("ansi_array.rs"))?;

    let numbers: Vec<u8> = (0..=u8::MAX).collect();

    write!(file, "{}", quote! {
        pub const U8_TO_STR: [&str; 256] = [
            #( concat!(#numbers, ";") ),*
        ];
    })
}
