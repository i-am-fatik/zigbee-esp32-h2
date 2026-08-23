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

use zigbee::{
    APPLICATION, CLUSTER_BASIC, CLUSTER_COLOUR_CONTROL, CLUSTER_GROUPS, CLUSTER_IDENTIFY,
    CLUSTER_LEVEL_CONTROL, CLUSTER_ON_OFF, CLUSTER_OTA, CLUSTER_SCENES,
    COLOUR_TEMPERATURE_MIREDS,
};

struct Extend {
    import: &'static str,
    call: String,
}

/// One `light()` covers the switch, the brightness and whatever colour the
/// device serves, so its arguments are read off the cluster list rather than
/// written down.
fn light(colour: bool) -> String {
    let mut arguments = vec![
        "effect: false".to_string(),
        "powerOnBehavior: false".to_string(),
        "configureReporting: true".to_string(),
    ];
    if colour {
        arguments.push("color: {modes: ['hs']}".to_string());
        arguments.push(format!(
            "colorTemp: {{range: [{}, {}]}}",
            COLOUR_TEMPERATURE_MIREDS.start(),
            COLOUR_TEMPERATURE_MIREDS.end()
        ));
    }
    format!("light({{{}}})", arguments.join(", "))
}

fn extend_for(cluster: u16, dimmable: bool, colour: bool) -> Option<Extend> {
    match cluster {
        CLUSTER_IDENTIFY => Some(Extend {
            import: "identify",
            call: "identify()".to_string(),
        }),
        CLUSTER_ON_OFF if dimmable => None,
        CLUSTER_ON_OFF => Some(Extend {
            import: "onOff",
            call: "onOff({powerOnBehavior: false})".to_string(),
        }),
        CLUSTER_LEVEL_CONTROL => Some(Extend {
            import: "light",
            call: light(colour),
        }),
        _ => None,
    }
}

fn needs_no_extend_of_its_own(cluster: u16) -> bool {
    matches!(
        cluster,
        CLUSTER_BASIC | CLUSTER_ON_OFF | CLUSTER_COLOUR_CONTROL | CLUSTER_GROUPS | CLUSTER_SCENES
    )
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

    let dimmable = APPLICATION.clusters.contains(&CLUSTER_LEVEL_CONTROL);
    let colour = APPLICATION.clusters.contains(&CLUSTER_COLOUR_CONTROL);

    for &cluster in APPLICATION.clusters {
        match extend_for(cluster, dimmable, colour) {
            Some(extend) => {
                imports.insert(extend.import);
                calls.push(extend.call);
            }
            None if needs_no_extend_of_its_own(cluster) => {}
            None => unmapped.push(cluster),
        }
    }

    if APPLICATION.outputs.contains(&CLUSTER_OTA) {
        imports.insert("ota");
        calls.push("ota()".to_string());
    }

    let imports: Vec<_> = imports.into_iter().collect();
    let calls: Vec<_> = calls.iter().map(String::as_str).collect();
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
