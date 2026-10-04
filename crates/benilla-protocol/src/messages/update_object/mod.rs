//! `SMSG_UPDATE_OBJECT` body decode: the object list, each entry's [`MovementBlock`] and its
//! sparse [`ObjectFields`].

use std::io::{self, Read};

use crate::wire::{capacity_hint, read_packed_guid, read_u32_le, read_u8};

mod fields;
mod movement;

pub use fields::field;
pub use fields::{
    build_field_table, field_table, power_display_scale, quest_slot_state, CorpseLook, FieldTable,
    ObjectFields, OwnerFallback, PlayerSkillSlot, QuestLogSlot, UnitAuraSlot, AURA_FLAG_CANCELABLE,
    AURA_FLAG_EFF_INDEX_MASK, FIELDS_5875, FIELDS_8606, PLAYER_EXPLORED_ZONES_SLOTS,
    PLAYER_QUEST_LOG_SLOTS, PLAYER_SKILL_SLOTS, UNIT_AURA_POSITIVE_SLOTS, UNIT_AURA_SLOTS,
};
pub use movement::{CreateSpline, MovementBlock, MoverState, ObjectType};

/// One entry in an `SMSG_UPDATE_OBJECT` object list.
pub enum Object {
    Values {
        guid: u64,
        mask: ObjectFields,
    },
    Movement {
        guid: u64,
        movement: MovementBlock,
    },
    /// `CREATE_OBJECT` or `CREATE_OBJECT2`, which share one wire shape.
    Create {
        guid: u64,
        object_type: ObjectType,
        movement: MovementBlock,
        mask: ObjectFields,
    },
    OutOfRange {
        guids: Vec<u64>,
    },
    Near {
        guids: Vec<u64>,
    },
}

/// Which build's movement block a body carries; framing, update types and the values block are
/// the same in both.
#[derive(Clone, Copy)]
enum Wire {
    Vanilla,
    Tbc,
}

impl Wire {
    fn movement(self, r: &mut impl Read) -> io::Result<MovementBlock> {
        match self {
            Wire::Vanilla => MovementBlock::read(r),
            Wire::Tbc => MovementBlock::read_tbc(r),
        }
    }
}

impl Object {
    fn read(r: &mut impl Read, table: &'static FieldTable, wire: Wire) -> io::Result<Self> {
        let update_type = read_u8(r)?;
        Ok(match update_type {
            0 => Object::Values {
                guid: read_packed_guid(r)?,
                mask: ObjectFields::read(r, table)?,
            },
            1 => Object::Movement {
                guid: read_packed_guid(r)?,
                movement: wire.movement(r)?,
            },
            2 | 3 => {
                let guid = read_packed_guid(r)?;
                let object_type = ObjectType::from_u8(read_u8(r)?);
                let movement = wire.movement(r)?;
                // A create omits zero fields (vmangos `_SetCreateBits`), so absent reads 0, but
                // only inside this type's own descriptor.
                let mask = ObjectFields::read(r, table)?.into_created(object_type);
                Object::Create {
                    guid,
                    object_type,
                    movement,
                    mask,
                }
            }
            4 => Object::OutOfRange {
                guids: read_guid_list(r)?,
            },
            5 => Object::Near {
                guids: read_guid_list(r)?,
            },
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown object update_type {other}"),
                ))
            }
        })
    }
}

fn read_guid_list(r: &mut impl Read) -> io::Result<Vec<u64>> {
    let count = read_u32_le(r)?;
    let mut guids = Vec::with_capacity(capacity_hint(count, 0xFFFF));
    for _ in 0..count {
        guids.push(read_packed_guid(r)?);
    }
    Ok(guids)
}

/// Parse an `SMSG_UPDATE_OBJECT` body: the count, the has-transport byte, then each `Object`.
pub(super) fn read_update_object(
    r: &mut impl Read,
    table: &'static FieldTable,
) -> io::Result<Vec<Object>> {
    read_objects(r, table, Wire::Vanilla)
}

/// [`read_update_object`] for a 2.4.3 body, through the 8606 table.
pub(super) fn read_update_object_tbc(r: &mut impl Read) -> io::Result<Vec<Object>> {
    read_objects(r, &FIELDS_8606, Wire::Tbc)
}

fn read_objects(
    r: &mut impl Read,
    table: &'static FieldTable,
    wire: Wire,
) -> io::Result<Vec<Object>> {
    let amount_of_objects = read_u32_le(r)?;
    let _has_transport = read_u8(r)?;
    let mut objects = Vec::with_capacity(capacity_hint(amount_of_objects, 0xFFFF));
    for _ in 0..amount_of_objects {
        objects.push(Object::read(r, table, wire)?);
    }
    Ok(objects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_log_slot_unpacks_counters_and_state() {
        // Slot 2 (198 + 6 = 204): counters 5/10/63/0 at 6-bit strides, state COMPLETE, a timer
        // (`Player.h:1100-1106`).
        let count_state =
            5u32 | (10 << 6) | (63 << 12) | ((quest_slot_state::COMPLETE as u32) << 24);
        let f = ObjectFields::from_pairs(&[(204, 783), (205, count_state), (206, 4242)]);
        assert_eq!(
            f.player_quest_log(2),
            Some(QuestLogSlot {
                quest_id: 783,
                counters: [5, 10, 63, 0],
                state: quest_slot_state::COMPLETE,
                timer: 4242,
            })
        );
        assert_eq!(f.player_quest_log(1), None);
        assert_eq!(f.player_quest_log(3), None);
        assert_eq!(f.player_quest_log(PLAYER_QUEST_LOG_SLOTS), None);
    }

    #[test]
    fn quest_log_cleared_slot_reads_zero_id() {
        // An abandoned slot: the server zeroes the id and the pair.
        let f = ObjectFields::from_pairs(&[(198, 0), (199, 0), (200, 0)]);
        assert_eq!(f.player_quest_log(0).map(|s| s.quest_id), Some(0));
    }
}
