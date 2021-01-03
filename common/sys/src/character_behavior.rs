use common::{
    comp::{
        Attacking, Beam, Body, CharacterState, Controller, Energy, Health, Loadout, Mounting, Ori,
        PhysicsState, Pos, StateUpdate, Vel,
    },
    event::{EventBus, LocalEvent, ServerEvent},
    metrics::SysMetrics,
    resources::DeltaTime,
    span,
    states::{
        self,
        behavior::{CharacterBehavior, JoinData, JoinTuple},
    },
    uid::{Uid, UidAllocator},
};

use specs::{Entities, Join, LazyUpdate, Read, ReadExpect, ReadStorage, System, WriteStorage};

fn incorporate_update(tuple: &mut JoinTuple, state_update: StateUpdate) {
    // TODO: if checking equality is expensive use optional field in StateUpdate
    if tuple.2.get_unchecked() != &state_update.character {
        *tuple.2.get_mut_unchecked() = state_update.character
    };
    *tuple.3 = state_update.pos;
    *tuple.4 = state_update.vel;
    *tuple.5 = state_update.ori;
    // Note: might be changed every tick by timer anyway
    if tuple.6.get_unchecked() != &state_update.energy {
        *tuple.6.get_mut_unchecked() = state_update.energy
    };
    if state_update.swap_loadout {
        let loadout = tuple.7.get_mut_unchecked();
        std::mem::swap(&mut loadout.active_item, &mut loadout.second_item);
    }
}

/// ## Character Behavior System
/// Passes `JoinData` to `CharacterState`'s `behavior` handler fn's. Receives a
/// `StateUpdate` in return and performs updates to ECS Components from that.
pub struct Sys;

impl<'a> System<'a> for Sys {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Entities<'a>,
        Read<'a, UidAllocator>,
        Read<'a, EventBus<ServerEvent>>,
        Read<'a, EventBus<LocalEvent>>,
        Read<'a, DeltaTime>,
        Read<'a, LazyUpdate>,
        ReadExpect<'a, SysMetrics>,
        WriteStorage<'a, CharacterState>,
        WriteStorage<'a, Pos>,
        WriteStorage<'a, Vel>,
        WriteStorage<'a, Ori>,
        WriteStorage<'a, Energy>,
        WriteStorage<'a, Loadout>,
        WriteStorage<'a, Controller>,
        ReadStorage<'a, Health>,
        ReadStorage<'a, Body>,
        ReadStorage<'a, PhysicsState>,
        ReadStorage<'a, Attacking>,
        ReadStorage<'a, Beam>,
        ReadStorage<'a, Uid>,
        ReadStorage<'a, Mounting>,
    );

    #[allow(clippy::while_let_on_iterator)] // TODO: Pending review in #587
    fn run(
        &mut self,
        (
            entities,
            _uid_allocator,
            server_bus,
            local_bus,
            dt,
            updater,
            sys_metrics,
            mut character_states,
            mut positions,
            mut velocities,
            mut orientations,
            mut energies,
            mut loadouts,
            mut controllers,
            healths,
            bodies,
            physics_states,
            attacking_storage,
            beam_storage,
            uids,
            mountings,
        ): Self::SystemData,
    ) {
        let start_time = std::time::Instant::now();
        span!(_guard, "run", "character_behavior::Sys::run");
        let mut server_emitter = server_bus.emitter();
        let mut local_emitter = local_bus.emitter();

        for mut tuple in (
            &entities,
            &uids,
            &mut character_states.restrict_mut(),
            &mut positions,
            &mut velocities,
            &mut orientations,
            &mut energies.restrict_mut(),
            &mut loadouts.restrict_mut(),
            &mut controllers,
            &healths,
            &bodies,
            &physics_states,
            attacking_storage.maybe(),
            beam_storage.maybe(),
        )
            .join()
        {
            // Being dead overrides all other states
            if tuple.9.is_dead {
                // Do nothing
                continue;
            }
            // If mounted, character state is controlled by mount
            // TODO: Make mounting a state
            if let Some(Mounting(_)) = mountings.get(tuple.0) {
                let sit_state = CharacterState::Sit {};
                if tuple.2.get_unchecked() != &sit_state {
                    *tuple.2.get_mut_unchecked() = sit_state;
                }
                continue;
            }

            let actions = std::mem::replace(&mut tuple.8.actions, Vec::new());
            for action in actions {
                let j = JoinData::new(&tuple, &updater, &dt);
                let mut state_update = match j.character {
                    CharacterState::Idle => states::idle::Data.handle_event(&j, action),
                    CharacterState::Climb => states::climb::Data.handle_event(&j, action),
                    CharacterState::Glide => states::glide::Data.handle_event(&j, action),
                    CharacterState::GlideWield => {
                        states::glide_wield::Data.handle_event(&j, action)
                    },
                    CharacterState::Sit => {
                        states::sit::Data::handle_event(&states::sit::Data, &j, action)
                    },
                    CharacterState::Dance => {
                        states::dance::Data::handle_event(&states::dance::Data, &j, action)
                    },
                    CharacterState::Sneak => {
                        states::sneak::Data::handle_event(&states::sneak::Data, &j, action)
                    },
                    CharacterState::BasicBlock => {
                        states::basic_block::Data.handle_event(&j, action)
                    },
                    CharacterState::Roll(data) => data.handle_event(&j, action),
                    CharacterState::Wielding => states::wielding::Data.handle_event(&j, action),
                    CharacterState::Equipping(data) => data.handle_event(&j, action),
                    CharacterState::ComboMelee(data) => data.handle_event(&j, action),
                    CharacterState::BasicMelee(data) => data.handle_event(&j, action),
                    CharacterState::BasicRanged(data) => data.handle_event(&j, action),
                    CharacterState::Boost(data) => data.handle_event(&j, action),
                    CharacterState::DashMelee(data) => data.handle_event(&j, action),
                    CharacterState::LeapMelee(data) => data.handle_event(&j, action),
                    CharacterState::SpinMelee(data) => data.handle_event(&j, action),
                    CharacterState::ChargedMelee(data) => data.handle_event(&j, action),
                    CharacterState::ChargedRanged(data) => data.handle_event(&j, action),
                    CharacterState::RepeaterRanged(data) => data.handle_event(&j, action),
                    CharacterState::Shockwave(data) => data.handle_event(&j, action),
                    CharacterState::BasicBeam(data) => data.handle_event(&j, action),
                };
                local_emitter.append(&mut state_update.local_events);
                server_emitter.append(&mut state_update.server_events);
                incorporate_update(&mut tuple, state_update);
            }

            let j = JoinData::new(&tuple, &updater, &dt);

            let mut state_update = match j.character {
                CharacterState::Idle => states::idle::Data.behavior(&j),
                CharacterState::Climb => states::climb::Data.behavior(&j),
                CharacterState::Glide => states::glide::Data.behavior(&j),
                CharacterState::GlideWield => states::glide_wield::Data.behavior(&j),
                CharacterState::Sit => states::sit::Data::behavior(&states::sit::Data, &j),
                CharacterState::Dance => states::dance::Data::behavior(&states::dance::Data, &j),
                CharacterState::Sneak => states::sneak::Data::behavior(&states::sneak::Data, &j),
                CharacterState::BasicBlock => states::basic_block::Data.behavior(&j),
                CharacterState::Roll(data) => data.behavior(&j),
                CharacterState::Wielding => states::wielding::Data.behavior(&j),
                CharacterState::Equipping(data) => data.behavior(&j),
                CharacterState::ComboMelee(data) => data.behavior(&j),
                CharacterState::BasicMelee(data) => data.behavior(&j),
                CharacterState::BasicRanged(data) => data.behavior(&j),
                CharacterState::Boost(data) => data.behavior(&j),
                CharacterState::DashMelee(data) => data.behavior(&j),
                CharacterState::LeapMelee(data) => data.behavior(&j),
                CharacterState::SpinMelee(data) => data.behavior(&j),
                CharacterState::ChargedMelee(data) => data.behavior(&j),
                CharacterState::ChargedRanged(data) => data.behavior(&j),
                CharacterState::RepeaterRanged(data) => data.behavior(&j),
                CharacterState::Shockwave(data) => data.behavior(&j),
                CharacterState::BasicBeam(data) => data.behavior(&j),
            };

            local_emitter.append(&mut state_update.local_events);
            server_emitter.append(&mut state_update.server_events);
            incorporate_update(&mut tuple, state_update);
        }
        sys_metrics.character_behavior_ns.store(
            start_time.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}