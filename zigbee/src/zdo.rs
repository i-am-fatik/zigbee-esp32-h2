use crate::buf::{Reader, Writer};

pub const NWK_ADDR_REQ: u16 = 0x0000;
pub const IEEE_ADDR_REQ: u16 = 0x0001;
pub const NODE_DESC_REQ: u16 = 0x0002;
pub const POWER_DESC_REQ: u16 = 0x0003;
pub const SIMPLE_DESC_REQ: u16 = 0x0004;
pub const ACTIVE_EP_REQ: u16 = 0x0005;
pub const MATCH_DESC_REQ: u16 = 0x0006;
pub const DEVICE_ANNCE: u16 = 0x0013;
pub const BIND_REQ: u16 = 0x0021;
pub const UNBIND_REQ: u16 = 0x0022;
pub const MGMT_LQI_REQ: u16 = 0x0031;
pub const MGMT_LEAVE_REQ: u16 = 0x0034;
pub const MGMT_PERMIT_JOINING_REQ: u16 = 0x0036;

pub const RESPONSE_BIT: u16 = 0x8000;

pub const STATUS_SUCCESS: u8 = 0x00;
pub const STATUS_NOT_SUPPORTED: u8 = 0x84;

pub const ENDPOINT: u8 = 1;
pub const DEVICE_ID_COLOUR_LIGHT: u16 = 0x0102;

pub const CLUSTER_BASIC: u16 = 0x0000;
pub const CLUSTER_IDENTIFY: u16 = 0x0003;
pub const CLUSTER_GROUPS: u16 = 0x0004;
pub const CLUSTER_SCENES: u16 = 0x0005;
pub const CLUSTER_ON_OFF: u16 = 0x0006;
pub const CLUSTER_LEVEL_CONTROL: u16 = 0x0008;
pub const CLUSTER_COLOUR_CONTROL: u16 = 0x0300;

pub(crate) const INPUT_CLUSTERS: [u16; 7] = [
    CLUSTER_BASIC,
    CLUSTER_IDENTIFY,
    CLUSTER_GROUPS,
    CLUSTER_SCENES,
    CLUSTER_ON_OFF,
    CLUSTER_LEVEL_CONTROL,
    CLUSTER_COLOUR_CONTROL,
];

/// The upgrade cluster sits the other way round: the light is the client and
/// the coordinator serves the images, so it is listed as an output.
pub(crate) const OUTPUT_CLUSTERS: [u16; 1] = [super::ota::CLUSTER];

pub fn device_announce(out: &mut Writer, seq: u8, short: u16, ieee: u64, capability: u8) {
    out.u8(seq);
    out.u16(short);
    out.u64(ieee);
    out.u8(capability);
}

/// A logical end device on mains power, on the 2.4 GHz band, with the buffer
/// sizes a small stack can actually honour.
fn node_descriptor(out: &mut Writer, capability: u8) {
    out.u8(0x02);
    out.u8(0x40);
    out.u8(capability);
    out.u16(0x1037);
    out.u8(82);
    out.u16(82);
    out.u16(0x0000);
    out.u16(82);
    out.u8(0x00);
}

fn power_descriptor(out: &mut Writer) {
    out.u8(0x10);
    out.u8(0xc1);
}

fn simple_descriptor(out: &mut Writer) {
    out.u16(super::aps::PROFILE_HOME_AUTOMATION);
    out.u16(DEVICE_ID_COLOUR_LIGHT);
    out.u8(0x01);
    out.u8(INPUT_CLUSTERS.len() as u8);
    for cluster in INPUT_CLUSTERS {
        out.u16(cluster);
    }
    out.u8(OUTPUT_CLUSTERS.len() as u8);
    for cluster in OUTPUT_CLUSTERS {
        out.u16(cluster);
    }
}

pub struct Response {
    pub cluster: u16,
}

/// The two requests a device has to answer even when they arrive addressed to
/// the whole network, because they are how a coordinator finds it in the first
/// place. Both answer only for the device actually being asked about.
fn answerable_when_broadcast(cluster: u16) -> bool {
    matches!(cluster, NWK_ADDR_REQ | MATCH_DESC_REQ)
}

/// Answers a device-object request. Returns the cluster the reply belongs to,
/// or `None` when the request needs no answer.
pub fn respond(
    out: &mut Writer,
    cluster: u16,
    request: &[u8],
    short: u16,
    ieee: u64,
    capability: u8,
    broadcast: bool,
) -> Option<Response> {
    if broadcast && !answerable_when_broadcast(cluster) {
        return None;
    }

    let mut r = Reader::new(request);
    let seq = r.u8()?;

    match cluster {
        NODE_DESC_REQ => {
            let of_interest = r.u16()?;
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
            out.u16(of_interest);
            node_descriptor(out, capability);
        }
        POWER_DESC_REQ => {
            let of_interest = r.u16()?;
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
            out.u16(of_interest);
            power_descriptor(out);
        }
        ACTIVE_EP_REQ => {
            let of_interest = r.u16()?;
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
            out.u16(of_interest);
            out.u8(1);
            out.u8(ENDPOINT);
        }
        SIMPLE_DESC_REQ => {
            let of_interest = r.u16()?;
            let endpoint = r.u8()?;
            out.u8(seq);
            if endpoint != ENDPOINT {
                out.u8(0x82);
                out.u16(of_interest);
                out.u8(0);
            } else {
                out.u8(STATUS_SUCCESS);
                out.u16(of_interest);
                let length_at = out.len();
                out.u8(0);
                out.u8(ENDPOINT);
                let body_start = out.len();
                simple_descriptor(out);
                let length = (out.len() - body_start + 1) as u8;
                out.set(length_at, length);
            }
        }
        MATCH_DESC_REQ => {
            let of_interest = r.u16()?;
            let profile = r.u16()?;
            let wanted = r.u8()? as usize;
            let mut matches = profile == super::aps::PROFILE_HOME_AUTOMATION && wanted == 0;
            for _ in 0..wanted {
                if INPUT_CLUSTERS.contains(&r.u16()?) {
                    matches = true;
                }
            }
            if broadcast && !matches {
                return None;
            }
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
            out.u16(of_interest);
            if matches {
                out.u8(1);
                out.u8(ENDPOINT);
            } else {
                out.u8(0);
            }
        }
        IEEE_ADDR_REQ => {
            let _of_interest = r.u16()?;
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
            out.u64(ieee);
            out.u16(short);
            out.u8(0);
        }
        NWK_ADDR_REQ => {
            if r.u64()? != ieee {
                return None;
            }
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
            out.u64(ieee);
            out.u16(short);
            out.u8(0);
        }
        BIND_REQ | UNBIND_REQ => {
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
        }
        MGMT_LEAVE_REQ | MGMT_PERMIT_JOINING_REQ => {
            out.u8(seq);
            out.u8(STATUS_SUCCESS);
        }
        MGMT_LQI_REQ => {
            out.u8(seq);
            out.u8(STATUS_NOT_SUPPORTED);
        }
        _ => return None,
    }

    Some(Response {
        cluster: cluster | RESPONSE_BIT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: u16 = 0x4560;
    const IEEE: u64 = 0x0011_2233_4455_6677;
    const CAPABILITY: u8 = 0x8c;

    fn ask(cluster: u16, request: &[u8]) -> (u16, Vec<u8>) {
        let mut buffer = [0u8; 96];
        let mut out = Writer::new(&mut buffer);
        let response = respond(&mut out, cluster, request, SHORT, IEEE, CAPABILITY, false)
            .expect("a coordinator asked and is owed an answer");
        let len = out.len();
        (response.cluster, buffer[..len].to_vec())
    }

    #[test]
    fn the_active_endpoints_are_the_one_the_light_answers_on() {
        let (cluster, reply) = ask(ACTIVE_EP_REQ, &[0x42, 0x60, 0x45]);

        assert_eq!(cluster, ACTIVE_EP_REQ | RESPONSE_BIT);
        assert_eq!(reply, vec![0x42, STATUS_SUCCESS, 0x60, 0x45, 1, ENDPOINT]);
    }

    #[test]
    fn the_simple_descriptor_describes_a_colour_light() {
        let (cluster, reply) = ask(SIMPLE_DESC_REQ, &[0x43, 0x60, 0x45, ENDPOINT]);

        assert_eq!(cluster, SIMPLE_DESC_REQ | RESPONSE_BIT);
        assert_eq!(reply[0], 0x43);
        assert_eq!(reply[1], STATUS_SUCCESS);
        assert_eq!(reply[4] as usize, reply.len() - 5);
        assert_eq!(reply[5], ENDPOINT);
        assert_eq!(
            u16::from_le_bytes([reply[8], reply[9]]),
            DEVICE_ID_COLOUR_LIGHT
        );
        assert_eq!(reply[11] as usize, INPUT_CLUSTERS.len());
        assert_eq!(
            reply[12 + 2 * INPUT_CLUSTERS.len()] as usize,
            OUTPUT_CLUSTERS.len()
        );
    }

    #[test]
    fn a_descriptor_for_an_endpoint_the_light_has_not_got_is_refused() {
        let (_, reply) = ask(SIMPLE_DESC_REQ, &[0x44, 0x60, 0x45, 9]);

        assert_ne!(reply[1], STATUS_SUCCESS);
    }
}
