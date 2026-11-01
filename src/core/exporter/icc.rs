//! Builds a minimal but valid sRGB ICC v2 profile at runtime, so the exe embeds a real colour
//! profile without shipping an external `.icc` asset. Primaries/white point are the canonical
//! D50-adapted sRGB values; the tone curve is the exact sRGB EOTF as a 1024-point 'curve' tag.

pub fn srgb_profile() -> Vec<u8> {
    // Tag payloads first, so we can compute offsets.
    let desc = text_desc_tag("sRGB");
    let cprt = text_tag("Public Domain");
    let wtpt = xyz_tag(0.9642, 1.0, 0.8249);
    let r_xyz = xyz_tag(0.43607, 0.22249, 0.01392);
    let g_xyz = xyz_tag(0.38515, 0.71687, 0.09708);
    let b_xyz = xyz_tag(0.14307, 0.06061, 0.71410);
    let trc = srgb_curve_tag();

    // Tag table: (signature, payload).
    let entries: [(&[u8; 4], &Vec<u8>); 8] = [
        (b"desc", &desc),
        (b"wtpt", &wtpt),
        (b"rXYZ", &r_xyz),
        (b"gXYZ", &g_xyz),
        (b"bXYZ", &b_xyz),
        (b"rTRC", &trc),
        (b"gTRC", &trc),
        (b"bTRC", &trc),
    ];
    let (cprt_sig, cprt_payload): (&[u8; 4], &Vec<u8>) = (b"cprt", &cprt);

    let mut all: Vec<(&[u8; 4], &Vec<u8>)> = entries.to_vec();
    all.push((cprt_sig, cprt_payload));
    all.sort_by_key(|(sig, _)| **sig); // ICC requires the tag table in ascending signature order

    let tag_count = all.len();
    let header_len = 128usize;
    let table_len = 4 + tag_count * 12;

    // Lay out each distinct payload once in `blob`, recording its offset; rTRC/gTRC/bTRC share one
    // block (added once, referenced thrice).
    let mut blob = Vec::new();
    let mut trc_offset: Option<usize> = None;
    let mut table = Vec::new();
    for (sig, payload) in &all {
        let is_trc = *sig == b"rTRC" || *sig == b"gTRC" || *sig == b"bTRC";
        let off = if is_trc && trc_offset.is_some() {
            trc_offset.unwrap()
        } else {
            let o = header_len + table_len + blob.len();
            if is_trc {
                trc_offset = Some(o);
            }
            blob.extend_from_slice(payload);
            pad4(&mut blob);
            o
        };
        table.extend_from_slice(*sig);
        table.extend_from_slice(&(off as u32).to_be_bytes());
        table.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    }

    let total = header_len + table_len + blob.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&header(total));
    out.extend_from_slice(&(tag_count as u32).to_be_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&blob);
    out
}

fn header(total: usize) -> [u8; 128] {
    let mut h = [0u8; 128];
    h[0..4].copy_from_slice(&(total as u32).to_be_bytes()); // profile size
    h[8..12].copy_from_slice(&[0x02, 0x10, 0x00, 0x00]); // profile version 2.1.0
    h[12..16].copy_from_slice(b"mntr"); // device class: display
    h[16..20].copy_from_slice(b"RGB "); // colour space
    h[20..24].copy_from_slice(b"XYZ "); // PCS
    h[36..40].copy_from_slice(b"acsp"); // signature
    h[40..44].copy_from_slice(b"MSFT"); // primary platform
    // Creation date/time (dateTimeNumber): a fixed valid stamp beats the all-zero one some
    // readers treat as a malformed profile.
    h[24..36].copy_from_slice(&[0x07, 0xE6, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]); // 2022-01-01 00:00:00
    // PCS illuminant = D50.
    h[68..72].copy_from_slice(&s15fixed16(0.9642).to_be_bytes());
    h[72..76].copy_from_slice(&s15fixed16(1.0).to_be_bytes());
    h[76..80].copy_from_slice(&s15fixed16(0.8249).to_be_bytes());
    h
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut v = Vec::with_capacity(20);
    v.extend_from_slice(b"XYZ ");
    v.extend_from_slice(&[0u8; 4]);
    v.extend_from_slice(&s15fixed16(x).to_be_bytes());
    v.extend_from_slice(&s15fixed16(y).to_be_bytes());
    v.extend_from_slice(&s15fixed16(z).to_be_bytes());
    v
}

// 1024-entry 'curve' tag encoding the exact sRGB EOTF (u16 values).
fn srgb_curve_tag() -> Vec<u8> {
    const N: usize = 1024;
    let mut v = Vec::with_capacity(12 + N * 2);
    v.extend_from_slice(b"curv");
    v.extend_from_slice(&[0u8; 4]);
    v.extend_from_slice(&(N as u32).to_be_bytes());
    for i in 0..N {
        let x = i as f64 / (N as f64 - 1.0);
        let linear = if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        };
        let u = (linear * 65535.0).round().clamp(0.0, 65535.0) as u16;
        v.extend_from_slice(&u.to_be_bytes());
    }
    v
}

// 'desc' tag (ICC v2 textDescription).
fn text_desc_tag(s: &str) -> Vec<u8> {
    let ascii = s.as_bytes();
    let count = ascii.len() as u32 + 1; // include NUL
    let mut v = Vec::new();
    v.extend_from_slice(b"desc");
    v.extend_from_slice(&[0u8; 4]);
    v.extend_from_slice(&count.to_be_bytes());
    v.extend_from_slice(ascii);
    v.push(0);
    // Unicode + scriptcode blocks (unused, but their fields are mandatory and fixed-width).
    v.extend_from_slice(&[0u8; 4]); // unicode language code
    v.extend_from_slice(&[0u8; 4]); // unicode count
    v.extend_from_slice(&[0u8; 3]); // scriptcode code(2) + count(1)
    // Macintosh scriptcode description (67 bytes, zeroed).
    v.extend_from_slice(&[0u8; 67]);
    v
}

// 'text' tag (ICC v2 textType) - the required type for `cprt`.
fn text_tag(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"text");
    v.extend_from_slice(&[0u8; 4]);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
    v
}

fn s15fixed16(v: f64) -> i32 {
    (v * 65536.0).round() as i32
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

#[cfg(test)]
mod tests {
    // Photoshop rejects a profile that trips any of these; each one was a real bug here.
    #[test]
    fn profile_is_structurally_valid() {
        let p = super::srgb_profile();
        let be32 = |o: usize| u32::from_be_bytes(p[o..o + 4].try_into().unwrap()) as usize;

        assert_eq!(be32(0), p.len(), "declared size must match");
        assert_eq!(&p[8..12], &[0x02, 0x10, 0x00, 0x00], "version must not be zero");
        assert_eq!(&p[36..40], b"acsp");

        let count = be32(128);
        let mut prev = [0u8; 4];
        for i in 0..count {
            let e = 132 + i * 12;
            let sig: [u8; 4] = p[e..e + 4].try_into().unwrap();
            let (off, len) = (be32(e + 4), be32(e + 8));
            assert!(sig > prev, "tag table must ascend by signature");
            prev = sig;
            assert!(off + len <= p.len(), "tag must lie inside the profile");

            match &sig {
                // cprt is textType, not textDescriptionType.
                b"cprt" => assert_eq!(&p[off..off + 4], b"text"),
                // textDescription is 90 bytes of fixed fields plus the ASCII string.
                b"desc" => assert_eq!(len, 90 + be32(off + 8)),
                _ => {}
            }
        }
        for tag in [b"desc", b"cprt", b"wtpt", b"rXYZ", b"gXYZ", b"bXYZ", b"rTRC", b"gTRC", b"bTRC"] {
            assert!(
                (0..count).any(|i| &p[132 + i * 12..136 + i * 12] == tag),
                "missing required tag {}",
                std::str::from_utf8(tag).unwrap()
            );
        }
    }
}
