//! Prints an install code the way it has to be given to a coordinator.
//!
//! ```text
//! cargo run --example install-code -- 83FED3407A939723A5C639B26916D505
//! ```

use zigbee::INSTALL_CODE_LEN;

fn parse(argument: &str) -> Result<[u8; INSTALL_CODE_LEN], String> {
    if argument.len() != INSTALL_CODE_LEN * 2 {
        return Err(format!(
            "an install code is {INSTALL_CODE_LEN} octets, so {} hexadecimal digits, not {}",
            INSTALL_CODE_LEN * 2,
            argument.len()
        ));
    }

    let mut code = [0u8; INSTALL_CODE_LEN];
    for (octet, pair) in code.iter_mut().zip(argument.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| "that is not hexadecimal".to_string())?;
        *octet = u8::from_str_radix(pair, 16).map_err(|_| format!("{pair} is not hexadecimal"))?;
    }
    Ok(code)
}

fn main() {
    let code = match std::env::args().nth(1).as_deref().map(parse) {
        Some(Ok(code)) => code,
        Some(Err(problem)) => {
            eprintln!("{problem}");
            std::process::exit(2);
        }
        None => {
            eprintln!(
                "usage: install-code <{} hexadecimal digits>",
                INSTALL_CODE_LEN * 2
            );
            std::process::exit(2);
        }
    };

    for octet in zigbee::install_code_label(&code) {
        print!("{octet:02X}");
    }
    println!();
    eprintln!("note: the last four digits are the checksum, the rest is the secret");
}
