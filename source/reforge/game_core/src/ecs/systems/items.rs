//! Dominio ITEMS del mundo (C5): los métodos del `WorldSim` de los items del
//! suelo (`process_drop`/`process_pickup` + helpers). El sistema de pickup
//! automático del lane futuro crecerá aquí.

use crate::ecs::components::{Item, Position, Vid};
use crate::ecs::events::{ItemEvent, ItemView, NpcEvent};
use crate::ecs::resources::{ItemIndex, VidAlloc};
use crate::ecs::world::WorldSim;

impl WorldSim {
    /// Estado del item del suelo `vid` (None si no existe).
    fn item_view(&self, vid: u32) -> Option<ItemView> {
        let e = *self.world.resource::<ItemIndex>().0.get(&vid)?;
        let ent = self.world.get_entity(e).ok()?;
        let pos = ent.get::<Position>()?;
        let item = ent.get::<Item>()?;
        Some(ItemView {
            vnum: item.vnum,
            count: item.count,
            x: pos.x,
            y: pos.y,
            z: item.z,
            sockets: item.sockets,
            attrs: item.attrs,
        })
    }

    /// Quita el item del suelo (commit del pickup). Idempotente.
    pub(crate) fn remove_item(&mut self, vid: u32) {
        if let Some(e) = self.world.resource_mut::<ItemIndex>().0.remove(&vid)
            && self.world.get_entity(e).is_ok()
        {
            self.world.despawn(e);
        }
    }

    /// Drop del kill: el mundo asigna el vid del item (VidAlloc — GLOBAL),
    /// crea la entidad y responde `DropResult` (el canal manda el
    /// GC_ITEM_GROUND_ADD + GC_ITEM_OWNERSHIP con el vid asignado).
    // 9 parámetros = el evento de kill completo (vid, vnum, count, posición
    // y rolleo de sockets/attrs) — un struct añadiría ruido al call-site.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process_drop(
        &mut self,
        player_vid: u32,
        vnum: u32,
        count: u32,
        x: i32,
        y: i32,
        z: i32,
        sockets: [i64; 3],
        attrs: [(i16, i16); 7],
    ) -> Vec<NpcEvent> {
        let item_vid = self.world.resource_mut::<VidAlloc>().item;
        self.world.resource_mut::<VidAlloc>().item += 1;
        let e = self
            .world
            .spawn((
                Vid { vid: item_vid },
                Position { x, y },
                Item {
                    vnum,
                    count,
                    z,
                    sockets,
                    attrs,
                },
            ))
            .id();
        self.world.resource_mut::<ItemIndex>().0.insert(item_vid, e);
        vec![
            ItemEvent::DropResult {
                player_vid,
                item_vid,
                vnum,
                count,
                x,
                y,
                z,
                sockets,
                attrs,
            }
            .into(),
        ]
    }

    /// Respuesta al pickup: el estado actual del item (None si ya no está —
    /// la distancia y el inventario los decide la conexión).
    pub(crate) fn process_pickup(&mut self, player_vid: u32, item_vid: u32) -> Vec<NpcEvent> {
        vec![
            ItemEvent::PickupResult {
                player_vid,
                item_vid,
                item: self.item_view(item_vid),
            }
            .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::events::{ItemEvent, ItemIntent};
    use crate::ecs::test_util::*;

    /// Ciclo de vida del item del suelo por intents: drop (vid del mundo) →
    /// pickup (reporta attrs/sockets) → remove (commit) → pickup (None).
    #[test]
    fn item_drop_pickup_remove() {
        let mut w = world_with(42);
        let sockets = [1i64, 0, 0];
        let attrs = [
            (1i16, 10i16),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (53, 3),
            (0, 0),
        ];
        let events = w.process_intent(
            ItemIntent::DropItem {
                player_vid: 2,
                vnum: 101,
                count: 1,
                x: 100,
                y: 200,
                z: 0,
                sockets,
                attrs,
            }
            .into(),
            1_000,
        );
        let drop = events.iter().find_map(|e| match e {
            NpcEvent::Item(ItemEvent::DropResult {
                item_vid,
                vnum,
                count,
                x,
                y,
                z,
                ..
            }) => Some((*item_vid, *vnum, *count, *x, *y, *z)),
            _ => None,
        });
        assert_eq!(
            drop,
            Some((50_000, 101, 1, 100, 200, 0)),
            "vid del mundo (50 000+)"
        );
        let events = w.process_intent(
            ItemIntent::PickupItem {
                player_vid: 2,
                item_vid: 50_000,
            }
            .into(),
            2_000,
        );
        let item = events.iter().find_map(|e| match e {
            NpcEvent::Item(ItemEvent::PickupResult { item, .. }) => *item,
            _ => None,
        });
        assert_eq!(
            item,
            Some(ItemView {
                vnum: 101,
                count: 1,
                x: 100,
                y: 200,
                z: 0,
                sockets,
                attrs
            }),
            "el pickup reporta los attrs/sockets del drop"
        );
        w.process_intent(ItemIntent::RemoveItem { item_vid: 50_000 }.into(), 3_000);
        let events = w.process_intent(
            ItemIntent::PickupItem {
                player_vid: 2,
                item_vid: 50_000,
            }
            .into(),
            4_000,
        );
        let item = events.iter().find_map(|e| match e {
            NpcEvent::Item(ItemEvent::PickupResult { item, .. }) => *item,
            _ => None,
        });
        assert_eq!(item, None, "el commit del pickup lo quitó del mundo");
    }

    /// Los vids NO colisionan entre tipos: los items van después de 50 000
    /// aunque los mobs sigan consumiendo (allocador GLOBAL del canal).
    #[test]
    fn vid_allocator_is_global_per_type() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join(&mut w);
        assert_eq!(w.npc_count(), 1);
        w.process_intent(
            ItemIntent::DropItem {
                player_vid: 2,
                vnum: 101,
                count: 1,
                x: 0,
                y: 0,
                z: 0,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            }
            .into(),
            1_000,
        );
        let events = w.process_intent(
            ItemIntent::DropItem {
                player_vid: 2,
                vnum: 101,
                count: 1,
                x: 0,
                y: 0,
                z: 0,
                sockets: [0; 3],
                attrs: [(0, 0); 7],
            }
            .into(),
            2_000,
        );
        let vid = events.iter().find_map(|e| match e {
            NpcEvent::Item(ItemEvent::DropResult { item_vid, .. }) => Some(*item_vid),
            _ => None,
        });
        assert_eq!(vid, Some(50_001), "el segundo drop sigue el rango de items");
    }
}
