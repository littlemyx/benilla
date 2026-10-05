//! 2.4.3 M2 layouts (versions 260-263), pinned on named models. Each fact was read from the bytes
//! with an independent reader; 108-byte bone records put every parent past the table.

use benilla_formats::Chain;
use benilla_m2::parse_m2;
use std::io::Cursor;

fn read(chain: &Chain, name: &str) -> Vec<u8> {
    chain.read(name).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn tbc_models_read_112_byte_bones_whose_parents_and_pivots_are_the_files() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    // (name, header version, bones, vertices, parents of bones 1..=6, bone 1's pivot z)
    let cases: [(&str, u32, usize, usize, [i16; 6], f32); 4] = [
        (
            "CHARACTER\\BloodElf\\Female\\BloodElfFemale.m2",
            263,
            157,
            5964,
            [0, 1, 1, 2, 3, 3],
            1.161_077_3,
        ),
        (
            "CHARACTER\\Draenei\\Male\\DraeneiMale.m2",
            263,
            148,
            5394,
            [0, 1, 1, 2, 2, 2],
            1.370_004_2,
        ),
        (
            "CREATURE\\AbyssalOutland\\Abyssal_Outland.m2",
            262,
            62,
            1794,
            [-1; 6],
            2.463_36,
        ),
        (
            "CREATURE\\AncientProtector\\AncientProtector.m2",
            260,
            74,
            970,
            [0, 1, 1, 2, 2, 2],
            -0.066_168_28,
        ),
    ];
    for (name, version, bones, verts, parents, pivot_z) in cases {
        let bytes = read(&chain, name);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            version,
            "{name}"
        );
        let fmt = parse_m2(&mut Cursor::new(&bytes[..])).expect(name);
        let m = fmt.model();
        assert_eq!(m.bones.len(), bones, "{name}");
        assert_eq!(m.vertices.len(), verts, "{name}");
        for (i, p) in parents.iter().enumerate() {
            assert_eq!(m.bones[i + 1].parent, *p, "{name} bone {}", i + 1);
        }
        assert!(near(m.bones[1].pivot.z, pivot_z), "{name} pivot");
        assert!(
            m.bones
                .iter()
                .all(|b| (-1..bones as i16).contains(&b.parent)),
            "{name}: a parent past the bone table"
        );
        // Four embedded views, each with 48-byte skin sections whose ranges tile the triangles.
        for i in 0..4 {
            let s = m.parse_embedded_skin(&bytes, i).expect(name);
            let mut next = 0;
            for sec in s.submeshes() {
                assert_eq!(sec.triangle_start, next, "{name} view {i}");
                next = sec.triangle_start + sec.triangle_count;
            }
            assert_eq!(next as usize, s.triangles().len(), "{name} view {i}");
        }
        assert!(m.parse_embedded_skin(&bytes, 4).is_err(), "{name}");
    }
}
