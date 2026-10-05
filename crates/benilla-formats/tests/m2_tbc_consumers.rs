//! 2.4.3 model readers beyond the parser crate (bone animation, particle emitters, texture
//! animation), pinned on named models. Every number was read from the bytes with the independent
//! python reader (`py_m2c.py`, `py_facts.py`), which agrees with these readers on all 14,695
//! models, 17,204,699 decoded rotation keys unit-norm within 1e-4.

use benilla_formats::{
    parse_m2_animations, parse_m2_bone_tracks, parse_m2_particle_emitters,
    parse_m2_render_submeshes, parse_m2_skeleton, Chain,
};

fn read(chain: &Chain, name: &str) -> Vec<u8> {
    chain.read(name).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

/// `(model, bones, Stand slot, start ms, end ms, head bone, rotation keys of it in the band,
/// first key)`: Stand is the first sequence with animation id 0; the head is the bone whose
/// KeyBoneID is 6.
type StandCase = (&'static str, usize, usize, u32, u32, usize, usize, [f32; 4]);

#[test]
fn the_player_models_stand_sequence_and_head_bone_rotation_keys() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    let cases: [StandCase; 2] = [
        (
            "CHARACTER\\BloodElf\\Female\\BloodElfFemale.m2",
            157,
            3,
            20_700,
            23_633,
            27,
            7,
            [-0.014_038_514, 0.170_049_13, -0.086_672_57, 0.981_505_8],
        ),
        (
            "CHARACTER\\Draenei\\Male\\DraeneiMale.m2",
            148,
            100,
            509_467,
            511_467,
            30,
            24,
            [0.018_341_625, 0.143_833_74, -0.018_463_7, 0.989_226_9],
        ),
    ];
    for (name, bones, slot, start, end, head, keys, first) in cases {
        let bytes = read(&chain, name);
        let skeleton = parse_m2_skeleton(&bytes).expect("skeleton");
        assert_eq!(skeleton.bones.len(), bones, "{name}");
        assert_eq!(skeleton.bones[head].key_bone, 6, "{name}: head bone");
        let stand = parse_m2_animations(&bytes)
            .into_iter()
            .find(|a| a.anim_id == 0)
            .expect("a Stand sequence");
        assert_eq!(
            (stand.seq_index, stand.start_ms, stand.end_ms),
            (slot, start, end),
            "{name}"
        );
        let rot = &stand
            .bones
            .iter()
            .find(|b| usize::from(b.bone) == head)
            .expect("the head is keyed in Stand")
            .rotation;
        assert_eq!(rot.len(), keys, "{name}: head rotation keys in Stand");
        assert!(rot[0].0.abs() < 1e-6, "{name}: first key at the band start");
        for (g, w) in rot[0].1.iter().zip(first) {
            assert!(near(*g, w), "{name}: {:?} vs {first:?}", rot[0].1);
        }
        for (_, q) in rot {
            let n = q.iter().map(|c| c * c).sum::<f32>().sqrt();
            assert!((n - 1.0).abs() < 2e-4, "{name}: unit quaternion, got {n}");
        }
        // The whole bone table, keys decoded: every rotation key is unit-norm.
        for (_, r, _) in parse_m2_bone_tracks(&bytes) {
            for (_, q) in r.keys {
                let n = q.iter().map(|c| c * c).sum::<f32>().sqrt();
                assert!((n - 1.0).abs() < 2e-4, "{name}: key norm {n}");
            }
        }
    }
}

#[test]
fn an_outland_creatures_particle_emitters_keep_their_shapes() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    // Header version 263: the emitter type is a u8 at +0x29 there.
    let bytes = read(&chain, "CREATURE\\ArcaneGolem\\ArcaneGolem.m2");
    let emitters = parse_m2_particle_emitters(&bytes).expect("emitters");
    let spheres = emitters
        .iter()
        .filter(|e| format!("{:?}", e.shape) == "Sphere")
        .count();
    // Six emitters, all spheres (type 2): read as u16 at +0x2a they were all planes.
    assert_eq!((emitters.len(), spheres), (6, 6));
}

#[test]
fn a_shoulder_robe_keeps_its_global_sequence_uv_scrolls() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    let bytes = read(
        &chain,
        "Item\\ObjectComponents\\SHOULDER\\LShoulder_Robe_D_01.m2",
    );
    let format = benilla_m2::parse_m2(&mut std::io::Cursor::new(&bytes[..])).expect("parse");
    let model = format.model();
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 262);
    assert_eq!(model.texture_transforms.len(), 3);
    // Three linear translation scrolls, one per global sequence 0..=2: (end ms, end x).
    for (t, (gseq, end, x)) in model.texture_transforms.iter().zip([
        (0u16, 5000u32, -2.0f32),
        (1, 6667, -2.0),
        (2, 2000, -1.0),
    ]) {
        let tr = &t.translation;
        assert_eq!((tr.interp, tr.gseq), (1, gseq));
        assert_eq!(tr.keys.len(), 2);
        assert_eq!((tr.keys[0].0, tr.keys[1].0), (0, end));
        assert_eq!(tr.keys[0].1, [0.0, 0.0, 0.0]);
        assert_eq!(tr.keys[1].1, [x, 0.0, 0.0]);
        assert!(t.rotation.keys.is_empty() && t.scaling.keys.is_empty());
    }
    // And the render path still builds the model's batches.
    assert!(!parse_m2_render_submeshes(&bytes, "", &[])
        .expect("submeshes")
        .is_empty());
}
