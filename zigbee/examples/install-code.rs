//! Prints an install code the way it has to be given to a coordinator.
//!
//! ```text
//! cargo run --example install-code -- 83FED3407A939723A5C639B26916D505
//! ```

fn main() {
    let Some(argument) = std::env::args().nth(1) else {
        eprintln!("usage: install-code <32 hexadecimal octets>");
        std::process::exit(2);
    };

    let digits: Vec<u8> = argument
        .as_bytes()
        .chunks(2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect();

    let Ok(code) = <[u8; 16]>::try_from(digits.as_slice()) else {
        eprintln!("an install code is 16 octets, which is 32 hexadecimal digits");
        std::process::exit(2);
    };

    for octet in zigbee::install_code_label(&code) {
        print!("{octet:02X}");
    }
    println!();
    eprintln!("note: the first 32 digits are the secret, the last 4 are its checksum");
}
