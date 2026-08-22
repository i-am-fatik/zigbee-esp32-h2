//! Writes the Zigbee2MQTT definition for the device this crate implements.
//!
//! A definition has two halves and they come from different places. The
//! behaviour half is derived here from the clusters the stack actually serves.
//! The identity half is whatever the coordinator saw during the interview, so
//! it is passed in rather than assumed - a definition keyed on a string the
//! device does not report is a file that silently never matches.
//!
//! ```text
//! cargo run --example zigbee2mqtt -- \
//!     --model H2.NoStd.Light --vendor esp-rs \
//!     --description "ESP32-H2 no_std Rust Zigbee light" > h2-nostd-light.mjs
//! ```

use std::collections::BTreeSet;

use zigbee::{APPLICATION, CLUSTER_BASIC, CLUSTER_IDENTIFY, CLUSTER_ON_OFF};

struct Extend {
    import: &'static str,
    call: &'static str,
}

fn extend_for(cluster: u16) -> Option<Extend> {
    match cluster {
        CLUSTER_IDENTIFY => Some(Extend {
            import: "identify",
            call: "identify()",
        }),
        CLUSTER_ON_OFF => Some(Extend {
            import: "onOff",
            call: "onOff({powerOnBehavior: false})",
        }),
        _ => None,
    }
}

struct Identity {
    model: String,
    vendor: String,
    description: String,
}

fn read_identity() -> Result<Identity, String> {
    let mut model = None;
    let mut vendor = None;
    let mut description = None;

    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments.next();
        match (flag.as_str(), value) {
            ("--model", Some(v)) => model = Some(v),
            ("--vendor", Some(v)) => vendor = Some(v),
            ("--description", Some(v)) => description = Some(v),
            (other, _) => return Err(format!("unknown argument {other}")),
        }
    }

    let model = model.ok_or("--model is the string the interview reported")?;
    let description = description.unwrap_or_else(|| model.clone());
    Ok(Identity {
        model,
        vendor: vendor.ok_or("--vendor is required")?,
        description,
    })
}

fn main() {
    let identity = match read_identity() {
        Ok(identity) => identity,
        Err(problem) => {
            eprintln!("{problem}");
            eprintln!(
                "usage: --model <interviewed model> --vendor <name> [--description <text>]"
            );
            std::process::exit(2);
        }
    };

    let mut imports = BTreeSet::new();
    let mut calls = Vec::new();
    let mut unmapped = Vec::new();

    for &cluster in APPLICATION.clusters {
        match extend_for(cluster) {
            Some(extend) => {
                imports.insert(extend.import);
                calls.push(extend.call);
            }
            None if cluster == CLUSTER_BASIC => {}
            None => unmapped.push(cluster),
        }
    }

    let imports: Vec<_> = imports.into_iter().collect();
    println!(
        "import {{{}}} from 'zigbee-herdsman-converters/lib/modernExtend';",
        imports.join(", ")
    );
    println!();
    println!("export default {{");
    println!("    zigbeeModel: ['{}'],", identity.model);
    println!("    model: '{}',", identity.model);
    println!("    vendor: '{}',", identity.vendor);
    println!("    description: '{}',", identity.description);
    println!("    extend: [{}],", calls.join(", "));
    println!("}};");

    for cluster in unmapped {
        eprintln!(
            "warning: cluster 0x{cluster:04x} is served but no extend describes it"
        );
    }
    eprintln!(
        "note: endpoint {}, profile 0x{:04x}, device 0x{:04x}",
        APPLICATION.endpoint, APPLICATION.profile, APPLICATION.device_id
    );
    eprintln!("note: exercise every extend before trusting it");
}
