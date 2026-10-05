//! The per-version record layout of an MD20 model: one description for every reader of raw model
//! bytes. Versions 256/257 are the 1.12.1 layout; 260-263 are the 2.4.3 one. Every size and offset
//! below was measured on the installs (a record size is right when each record's parent index and
//! track pointers land inside the file; a key size is right when decoded rotations are unit-norm).

use benilla_bytes::ByteExt;

/// First header version of the 2.x layout.
pub const FIRST_TBC_VERSION: u32 = 260;

/// A bone record's tracks are `M2Track`s of this many bytes in every version (interp u16, gseq u16,
/// then ranges, timestamps and values as `(count, offset)` pairs).
pub const TRACK_SIZE: usize = 0x1c;

/// Sizes and field offsets that depend on the header version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct M2Layout {
    pub version: u32,
    /// Bone record size: 108 in 256/257; 112 from 260 (a `boneNameCRC` u32 after `submesh`).
    pub bone_size: usize,
    /// Bone translation track offset: 0x0c, or 0x10 from 260.
    pub bone_translation: usize,
    /// Bone rotation track offset: 0x28, or 0x2c from 260.
    pub bone_rotation: usize,
    /// Bone scale track offset: 0x44, or 0x48 from 260.
    pub bone_scale: usize,
    /// Bone pivot offset: 0x60, or 0x64 from 260.
    pub bone_pivot: usize,
    /// Bone rotation key size: four f32 (16) in 256/257; four int16 (8) from 260.
    pub rotation_key_size: usize,
}

impl M2Layout {
    /// The layout of header `version` (anything below 260 is the 1.12.1 layout).
    pub const fn for_version(version: u32) -> Self {
        if version < FIRST_TBC_VERSION {
            Self {
                version,
                bone_size: 108,
                bone_translation: 0x0c,
                bone_rotation: 0x28,
                bone_scale: 0x44,
                bone_pivot: 0x60,
                rotation_key_size: 16,
            }
        } else {
            Self {
                version,
                bone_size: 112,
                bone_translation: 0x10,
                bone_rotation: 0x2c,
                bone_scale: 0x48,
                bone_pivot: 0x64,
                rotation_key_size: 8,
            }
        }
    }

    /// The layout of the model whose bytes these are; a file too short for a header reads as 1.12.1.
    pub fn of(bytes: &[u8]) -> Self {
        Self::for_version(bytes.u32_at(4).unwrap_or(256))
    }

    /// Whether bone rotation keys are the compressed int16 form.
    pub const fn compressed_rotation(&self) -> bool {
        self.rotation_key_size == 8
    }

    /// One bone rotation key `[x, y, z, w]` at `o`, in this version's key form.
    pub fn read_rotation_key(&self, b: &[u8], o: usize) -> Option<[f32; 4]> {
        if self.compressed_rotation() {
            Some(decode_comp_quat([
                b.u16_at(o)? as i16,
                b.u16_at(o + 2)? as i16,
                b.u16_at(o + 4)? as i16,
                b.u16_at(o + 6)? as i16,
            ]))
        } else {
            Some([
                b.f32_at(o)?,
                b.f32_at(o + 4)?,
                b.f32_at(o + 8)?,
                b.f32_at(o + 12)?,
            ])
        }
    }
}

/// One compressed quaternion component: `(v < 0 ? v + 32768 : v - 32767) / 32767`. Checked on the
/// 3,505,446 rotation keys of 2.4.3 models sampled: every decoded quaternion is unit-norm within
/// 1e-4 (plain `v / 32767` is not, on 133 of them).
pub fn decode_comp_quat_component(v: i16) -> f32 {
    let v = i32::from(v);
    (if v < 0 { v + 32768 } else { v - 32767 }) as f32 / 32767.0
}

/// A compressed `[x, y, z, w]` rotation key.
pub fn decode_comp_quat(q: [i16; 4]) -> [f32; 4] {
    q.map(decode_comp_quat_component)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_layouts_differ_by_the_name_crc_word() {
        let (old, new) = (M2Layout::for_version(256), M2Layout::for_version(263));
        assert_eq!((old.bone_size, new.bone_size), (108, 112));
        assert_eq!(M2Layout::for_version(257).bone_size, 108);
        assert!(!M2Layout::for_version(257).compressed_rotation());
        for v in 260..=263 {
            assert_eq!(M2Layout::for_version(v).bone_size, 112);
            assert_eq!(M2Layout::for_version(v).bone_rotation, 0x2c);
        }
        // Three 28-byte tracks end at the pivot, in both.
        for l in [old, new] {
            assert_eq!(l.bone_scale + TRACK_SIZE, l.bone_pivot);
            assert_eq!(l.bone_pivot + 12, l.bone_size);
            assert_eq!(l.bone_rotation, l.bone_translation + TRACK_SIZE);
        }
        assert_eq!((old.rotation_key_size, new.rotation_key_size), (16, 8));
    }

    #[test]
    fn compressed_components_map_the_int16_range_onto_unit_range() {
        assert_eq!(decode_comp_quat_component(-1), 1.0);
        assert_eq!(decode_comp_quat_component(i16::MAX), 0.0);
        assert_eq!(decode_comp_quat_component(i16::MIN), 0.0);
        assert_eq!(decode_comp_quat_component(0), -1.0);
        assert_eq!(
            decode_comp_quat([i16::MAX, i16::MIN, i16::MAX, -1]),
            [0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn unit_quaternions_survive_the_round_trip_within_one_step() {
        // Encode unit quaternions the way the int16 form stores them; decoded, they stay unit
        // length within the 1/32767 quantisation.
        let enc = |x: f32| -> i16 {
            let s = (x * 32767.0).round() as i32;
            (if s < 0 { s - 32768 } else { s + 32767 }) as i16
        };
        for i in 0..200 {
            let t = i as f32 * 0.37;
            let q = [
                t.sin(),
                (t * 1.7).cos(),
                (t * 0.3).sin(),
                0.8 + (t * 2.1).cos() * 0.2,
            ];
            let n = q.iter().map(|c| c * c).sum::<f32>().sqrt();
            let q = q.map(|c| c / n);
            let d = decode_comp_quat(q.map(enc));
            let dn = d.iter().map(|c| c * c).sum::<f32>().sqrt();
            assert!((dn - 1.0).abs() < 1e-3, "{d:?} norm {dn}");
            for (a, b) in q.iter().zip(d) {
                assert!((a - b).abs() < 1e-4, "{q:?} vs {d:?}");
            }
        }
    }

    #[test]
    fn rotation_keys_read_in_the_files_own_form() {
        let mut b = vec![0u8; 16];
        b[0..4].copy_from_slice(&0.5f32.to_le_bytes());
        b[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        assert_eq!(
            M2Layout::for_version(256).read_rotation_key(&b, 0),
            Some([0.5, 0.0, 0.0, 1.0])
        );
        let mut c = vec![0u8; 8];
        c[0..2].copy_from_slice(&i16::MAX.to_le_bytes());
        c[6..8].copy_from_slice(&(-1i16).to_le_bytes());
        assert_eq!(
            M2Layout::for_version(263).read_rotation_key(&c, 0),
            Some([0.0, -1.0, -1.0, 1.0])
        );
        assert_eq!(M2Layout::for_version(263).read_rotation_key(&c, 4), None);
    }
}
