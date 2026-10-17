#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_pose_becomes_a_ped_at_that_spot_facing_that_way() {
        let state = state_from_pose(relay::Pose { seq: 12, model: 0x1234, position: [10.0, 20.0, 30.0], rotation: [0.0, 95.0, 0.0] });
        assert_eq!(state.ped_model, 0x1234);
        assert_eq!(state.pos, [10.0, 20.0, 30.0]);
        assert_eq!(state.heading, 95.0, "yaw is the heading");
        assert_eq!(state.health, 100, "a relay peer is never reported dead");
        assert!(!state.dead());
        assert!(!state.in_vehicle(), "the relay says nothing about vehicles");
    }

    #[test]
    fn owner_numbers_are_stable_and_skip_zero() {
        let owners: Mutex<HashMap<String, u8>> = Mutex::new(HashMap::new());
        let next: Mutex<u8> = Mutex::new(0);
        let first = owner_for(&owners, &next, "aaaa");
        let again = owner_for(&owners, &next, "aaaa");
        let other = owner_for(&owners, &next, "bbbb");
        assert_eq!(first, again, "the same id must keep the same owner");
        assert_ne!(first, other, "different ids get different owners");
        assert_ne!(first, 0, "owner 0 is us");
        assert_ne!(other, 0);
    }

    #[test]
    fn a_forgotten_peer_frees_its_owner_number() {
        let owners: Mutex<HashMap<String, u8>> = Mutex::new(HashMap::new());
        let next: Mutex<u8> = Mutex::new(0);
        let a = owner_for(&owners, &next, "aaaa");
        assert_eq!(owners.lock().unwrap().remove("aaaa"), Some(a));
        let b = owner_for(&owners, &next, "bbbb");
        assert_ne!(a, b, "a new player must not inherit a stale owner");
    }

    #[test]
    fn short_ids_are_readable_but_not_the_whole_hex() {
        assert_eq!(short_id("deadbeefcafe0001"), "dead");
        assert_eq!(short_id(""), "");
    }
}// muguet - mp.rs
// Constantin Clerc (c22dev) - 0.1

use crate::proto::{self, Look, PopEntry, Shot, State, VehLook};
use crate::relay;
use crate::{read_cstr, read_u32, read_u64, write_u64, HostState};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use wasmtime::{Caller, Val};

#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub const COMMANDS: &[(&str, &str)] = &[
    ("PlayerPedId", "player_commands::CommandPlayerPedId()"),
    ("GetEntityCoords", "entity_commands::CommandGetEntityCoords(int, bool)"),
    ("GetEntityHeading", "entity_commands::CommandGetEntityHeading(int)"),
    ("GetEntityVelocity", "entity_commands::CommandGetEntityVelocity(int)"),
    ("GetEntityRotation", "entity_commands::CommandGetEntityRotation(int, int)"),
    ("GetEntityModel", "entity_commands::CommandGetEntityModel(int)"),
    ("GetEntityHealth", "entity_commands::CommandGetEntityHealth(int)"),
    ("SetEntityHealth", "entity_commands::CommandSetEntityHealth(int, int, int)"),
    ("FreezeEntityPosition", "entity_commands::CommandFreezeEntityPosition(int, bool)"),
    ("DoesEntityExist", "entity_commands::CommandDoesEntityExist(int)"),
    ("IsEntityAMissionEntity", "entity_commands::CommandIsEntityAMissionEntity(int)"),
    ("SetEntityCoordsNoOffset", "entity_commands::CommandSetEntityCoordsNoOffset(int, rage::scrVector const&, bool, bool, bool)"),
    ("SetEntityHeading", "entity_commands::CommandSetEntityHeading(int, float)"),
    ("SetEntityRotation", "entity_commands::CommandSetEntityRotation(int, rage::scrVector const&, int, bool)"),
    ("SetEntityVelocity", "entity_commands::CommandSetEntityVelocity(int, rage::scrVector const&)"),
    ("SetEntityInvincible", "entity_commands::CommandSetEntityInvincible(int, bool)"),
    ("SetEntityAsMissionEntity", "entity_commands::CommandSetEntityAsMissionEntity(int, bool, bool)"),
    ("DeleteEntity", "entity_commands::CommandDeleteEntity(int&)"),
    ("CreatePed", "ped_commands::CommandCreatePed(int, int, rage::scrVector const&, float, bool, bool)"),
    ("DeletePed", "ped_commands::CommandDeletePed(int&)"),
    ("GetVehiclePedIsIn", "ped_commands::CommandGetVehiclePedIsIn(int, bool)"),
    ("IsPedInAnyVehicle", "ped_commands::CommandIsPedInAnyVehicle(int, bool)"),
    ("IsPedDeadOrDying", "ped_commands::CommandIsPedDeadOrDying(int, bool)"),
    ("IsPedShooting", "ped_commands::CommandIsPedShooting(int)"),
    ("GetPedArmour", "ped_commands::CommandGetPedArmour(int)"),
    ("GetPedBoneCoords", "ped_commands::CommandGetPedBoneCoords(int, int, rage::scrVector const&)"),
    ("SetPedIntoVehicle", "ped_commands::CommandSetPedIntoVehicle(int, int, int)"),
    ("SetBlockingOfNonTemporaryEvents", "ped_commands::CommandSetBlockingOfNonTemporaryEvents(int, bool)"),
    ("SetPedCanRagdoll", "ped_commands::CommandSetPedCanRagdoll(int, bool)"),
    ("SetPedKeepTask", "ped_commands::CommandSetPedKeepTask(int, bool)"),
    ("SetPedCanBeDraggedOut", "ped_commands::CommandSetPedCanBeDraggedOut(int, bool)"),
    ("SetPedDefaultComponentVariation", "ped_commands::CommandSetPedDefaultComponentVariation(int)"),
    ("GetPedDrawableVariation", "ped_commands::CommandGetPedDrawableVariation(int, int)"),
    ("GetPedTextureVariation", "ped_commands::CommandGetPedTextureVariation(int, int)"),
    ("GetPedPaletteVariation", "ped_commands::CommandGetPedPaletteVariation(int, int)"),
    ("SetPedComponentVariation", "ped_commands::CommandSetPedComponentVariation(int, int, int, int, int)"),
    ("GetPedPropIndex", "ped_commands::CommandGetPedPropIndex(int, int)"),
    ("GetPropTextureIndex", "ped_commands::CommandGetPropTextureIndex(int, int)"),
    ("SetPedPropIndex", "ped_commands::CommandSetPedPropIndex(int, int, int, int, bool)"),
    ("ClearPedProp", "ped_commands::CommandClearPedProp(int, int)"),
    ("GetNearbyVehicles", "ped_commands::CommandGetNearbyVehicles(int, int&)"),
    ("GetNearbyPeds", "ped_commands::CommandGetNearbyPeds(int, int&, int)"),
    ("SetPedDensityMultiplierThisFrame", "ped_commands::CommandSetPedDensityMultiplierThisFrame(float)"),
    ("SetScenarioPedDensityMultiplierThisFrame", "ped_commands::CommandSetScenarioPedDensityMultiplierThisFrame(float, float)"),
    ("CreateVehicle", "vehicle_commands::CommandCreateVehicle(int, rage::scrVector const&, float, bool, bool, bool)"),
    ("DeleteVehicle", "vehicle_commands::CommandDeleteVehicle(int&)"),
    ("SetVehicleEngineOn", "vehicle_commands::CommandSetVehicleEngineOn(int, bool, bool, bool)"),
    ("GetPedInVehicleSeat", "vehicle_commands::CommandGetPedInVehicleSeat(int, int, bool)"),
    ("IsVehicleSeatFree", "vehicle_commands::CommandIsVehicleSeatFree(int, int, bool)"),
    ("GetVehicleColours", "vehicle_commands::CommandGetVehicleColours(int, int&, int&)"),
    ("SetVehicleColour", "vehicle_commands::CommandSetVehicleColour(int, int, int)"),
    ("SetVehicleDensityMultiplierThisFrame", "vehicle_commands::CommandSetVehicleDensityMultiplierThisFrame(float)"),
    ("SetRandomVehicleDensityMultiplierThisFrame", "vehicle_commands::CommandSetRandomVehicleDensityMultiplierThisFrame(float)"),
    ("SetParkedVehicleDensityMultiplierThisFrame", "vehicle_commands::CommandSetParkedVehicleDensityMultiplierThisFrame(float)"),
    ("ClearAreaOfVehicles", "misc_commands::CommandClearAreaOfVehicles(rage::scrVector const&, float, bool, bool, bool, bool, bool, bool, bool)"),
    ("ClearAreaOfPeds", "misc_commands::CommandClearAreaOfPeds(rage::scrVector const&, float, bool)"),
    ("FireSingleBulletIgnoreEntity", "misc_commands::CommandFireSingleBulletIgnoreEntity(rage::scrVector const&, rage::scrVector const&, int, bool, int, int, bool, bool, float, int, int)"),
    ("RequestWeaponAsset", "weapon_commands::RequestWeaponAsset(int, int, int)"),
    ("HasWeaponAssetLoaded", "weapon_commands::HasWeaponAssetLoaded(int)"),
    ("GetPlayerInvincible", "player_commands::CommandGetPlayerInvincible(int)"),
    ("GetEntityCanBeDamaged", "entity_commands::CommandGetEntityCanBeDamaged(int)"),
    ("GetVehicleEngineHealth", "vehicle_commands::CommandGetVehicleEngineHealth(int)"),
    ("SetVehicleEngineHealth", "vehicle_commands::CommandSetVehicleEngineHealth(int, float)"),
    ("GetVehicleBodyHealth", "vehicle_commands::CommandGetVehicleBodyHealth(int)"),
    ("SetVehicleBodyHealth", "vehicle_commands::CommandSetVehicleBodyHealth(int, float)"),
    ("GetVehiclePetrolTankHealth", "vehicle_commands::CommandGetVehiclePetrolTankHealth(int)"),
    ("SetVehiclePetrolTankHealth", "vehicle_commands::CommandSetVehiclePetrolTankHealth(int, float)"),
    ("IsVehicleTyreBurst", "vehicle_commands::CommandIsVehicleTyreBurst(int, int, bool)"),
    ("SetVehicleTyreBurst", "vehicle_commands::CommandSetVehicleTyreBurst(int, int, bool, float)"),
    ("IsVehicleWindowIntact", "vehicle_commands::CommandIsVehicleWindowIntact(int, int)"),
    ("SmashVehicleWindow", "vehicle_commands::CommandSmashVehicleWindow(int, int)"),
    ("ExplodeVehicle", "vehicle_commands::CommandExplodeVehicle(int, bool, bool)"),
    ("SetVehicleFixed", "vehicle_commands::CommandSetVehicleFixed(int)"),
    ("SetVehicleDeformationFixed", "vehicle_commands::CommandSetVehicleDeformationFixed(int)"),
    ("IsEntityDead", "entity_commands::CommandIsEntityDead(int, bool)"),
    ("IsEntityAPed", "entity_commands::CommandIsEntityAPed(int)"),
    ("IsEntityOnScreen", "entity_commands::CommandIsEntityOnScreen(int)"),
    ("SetEntityAsNoLongerNeeded", "entity_commands::CommandSetEntityAsNoLongerNeeded(int&)"),
    ("SetDrawOrigin", "graphics_commands::CommandSetDrawOrigin(rage::scrVector const&, bool)"),
    ("ClearDrawOrigin", "graphics_commands::CommandClearDrawOrigin()"),
    ("BeginTextCommandDisplayText", "hud_commands::CommandBeginTextCommandDisplayText(char const*)"),
    ("AddTextComponentSubStringPlayerName", "hud_commands::CommandAddTextComponentSubStringPlayerName(char const*)"),
    ("EndTextCommandDisplayText", "hud_commands::CommandEndTextCommandDisplayText(float, float, int)"),
    ("SetTextScale", "hud_commands::CommandSetTextScale(float, float)"),
    ("SetTextColour", "hud_commands::CommandSetTextColour(int, int, int, int)"),
    ("SetTextCentre", "hud_commands::CommandSetTextCentre(bool)"),
    ("SetTextFont", "hud_commands::CommandSetTextFont(int)"),
    ("SetTextOutline", "hud_commands::CommandSetTextOutline()"),
    ("DoesScriptExist", "script_commands::DoesScriptExist(char const*)"),
    ("AddRelationshipGroup", "ped_commands::CommandAddRelationshipGroup(char const*, int&)"),
    ("DoesRelationshipGroupExist", "ped_commands::CommandDoesRelationshipGroupExist(int)"),
    ("SetRelationshipBetweenGroups", "ped_commands::CommandSetRelationshipBetweenGroups(int, int, int)"),
    ("SetRelationshipGroupAffectsWantedLevel", "ped_commands::CommandSetRelationshipGroupAffectsWantedLevel(int, bool)"),
    ("GetPedRelationshipGroupHash", "ped_commands::CommandGetPedRelationshipGroupHash(int)"),
    ("SetPedRelationshipGroupHash", "ped_commands::CommandSetPedRelationshipGroupHash(int, int)"),
    ("SetPedCanBeTargetted", "ped_commands::CommandSetPedCanBeTargetted(int, bool)"),
    ("SetCanAttackFriendly", "ped_commands::CommandSetCanAttackFriendly(int, bool, bool)"),
    ("PlayerId", "player_commands::CommandPlayerId()"),
    ("IsPlayerFreeAiming", "player_commands::CommandIsPlayerFreeAiming(int)"),
    ("GetGameplayCamCoord", "camera_commands::CommandGetGameplayCamCoord()"),
    ("GetGameplayCamRot", "camera_commands::CommandGetGameplayCamRot(int)"),
    ("TaskGoToCoordWhileAimingAtCoord", "task_commands::CommandTaskGoToCoordWhileAimingAtCoord(int, rage::scrVector const&, rage::scrVector const&, float, bool, float, float, bool, int, bool, int)"),
    ("GetVehicleExtraColours", "vehicle_commands::CommandGetVehicleExtraColours(int, int&, int&)"),
    ("SetVehicleExtraColours", "vehicle_commands::CommandSetVehicleExtraColours(int, int, int)"),
    ("GetVehicleLivery", "vehicle_commands::CommandGetVehicleLivery(int)"),
    ("SetVehicleLivery", "vehicle_commands::CommandSetVehicleLivery(int, int)"),
    ("GetVehicleWindowTint", "vehicle_commands::CommandGetVehicleWindowTint(int)"),
    ("SetVehicleWindowTint", "vehicle_commands::CommandSetVehicleWindowTint(int, int)"),
    ("GetVehicleWheelType", "vehicle_commands::CommandGetVehicleWheelType(int)"),
    ("SetVehicleWheelType", "vehicle_commands::CommandSetVehicleWheelType(int, int)"),
    ("SetVehicleModKit", "vehicle_commands::CommandSetVehicleModKit(int, int)"),
    ("GetVehicleMod", "vehicle_commands::CommandGetVehicleMod(int, int)"),
    ("SetVehicleMod", "vehicle_commands::CommandSetVehicleMod(int, int, int, bool)"),
    ("IsToggleModOn", "vehicle_commands::CommandIsToggleModOn(int, int)"),
    ("ToggleVehicleMod", "vehicle_commands::CommandToggleVehicleMod(int, int, bool)"),
    ("GetVehicleNumberPlateText", "vehicle_commands::CommandGetVehicleNumberPlateText(int)"),
    ("SetVehicleNumberPlateText", "vehicle_commands::CommandSetVehicleNumberPlateText(int, char const*)"),
    ("GetPlayerRadioStationIndex", "audio_commands::CommandGetPlayerRadioStationIndex()"),
    ("RetuneRadioToStationIndex", "audio_commands::CommandRetuneRadioToStationIndex(int)"),
    ("GetBaseFromGuid", "rage::fwScriptGuid::GetBaseFromGuid(int)"),
    ("AddOwnedExplosion", "fire_commands::CommandAddOwnedExplosion(int, rage::scrVector const&, int, float, bool, bool, float)"),
    ("AddExplosion", "fire_commands::CommandAddExplosion(rage::scrVector const&, int, float, bool, bool, float, bool)"),
    ("SetEntityProofs", "entity_commands::CommandSetEntityProofs(int, bool, bool, bool, bool, bool, bool, bool, bool)"),
    ("SetPedArmour", "ped_commands::CommandSetPedArmour(int, int)"),
    ("SetTimeScale", "misc_commands::CommandSetTimeScale(float)"),
    ("IsControlPressed", "pad_commands::CommandIsControlPressed(int, int)"),
    ("SetClockTime", "clock_commands::CommandSetClockTime(int, int, int)"),
    ("GetClockHours", "clock_commands::CommandGetClockHours()"),
    ("GetClockMinutes", "clock_commands::CommandGetClockMinutes()"),
    ("GetClockSeconds", "clock_commands::CommandGetClockSeconds()"),
    ("GetNextWeatherTypeHashName", "misc_commands::CommandGetNextWeatherTypeHashName()"),
    ("SetWeatherTypeNowPersist", "misc_commands::CommandSetWeatherTypeNowPersist(char const*)"),
    ("HasEntityBeenDamagedByEntity", "entity_commands::CommandHasEntityBeenDamagedByEntity(int, int, bool)"),
    ("ClearEntityLastDamageEntity", "entity_commands::CommandClearEntityLastDamageEntity(int)"),
    ("GetEntitySpeed", "entity_commands::CommandGetEntitySpeed(int)"),
    ("GetMeleeTargetForPed", "ped_commands::CommandGetMeleeTargetForPed(int)"),
    ("DoesAnimDictExist", "streaming_commands::DoesAnimDictExist(char const*)"),
    ("HasAnimDictLoaded", "streaming_commands::HasAnimDictLoaded(char const*)"),
    ("RequestAnimDict", "streaming_commands::RequestAnimDict(char const*)"),
    ("GetAnimDuration", "entity_commands::CommandGetAnimDuration(char const*, char const*)"),
    ("TaskPlayAnim", "task_commands::CommandTaskPlayAnim(int, char const*, char const*, float, float, int, int, float, bool, int, bool)"),
    ("GetWeatherState", "misc_commands::CommandGetWeatherState(int&, int&, float&)"),
    ("SetWeatherState", "misc_commands::CommandSetWeatherState(int, int, float)"),
    ("ClearWeatherTypePersist", "misc_commands::CommandClearWeatherTypePersist()"),
    ("ClearPedTasks", "task_commands::CommandClearPedTasks(int)"),
    ("SetEntityAngularVelocity", "entity_commands::CommandSetEntityAngularVelocity(int, rage::scrVector const&)"),
    ("GetPedCauseOfDeath", "ped_commands::CommandGetPedCauseOfDeath(int)"),
    ("BeginTheFeedPost", "hud_commands::CommandBeginTheFeedPost(char const*)"),
    ("EndTheFeedPostTicker", "hud_commands::CommandEndTheFeedPostTicker(bool, bool)"),
    ("IsPlayerPressingHorn", "player_commands::CommandIsPlayerPressingHorn(int)"),
    ("IsPedRagdoll", "ped_commands::CommandIsPedRagdoll(int)"),
    ("TaskLeaveVehicle", "task_commands::CommandTaskLeaveVehicle(int, int, int)"),
    ("GetVehiclePedIsTryingToEnter", "ped_commands::CommandGetVehiclePedIsTryingToEnter(int)"),
    ("GetSeatPedIsTryingToEnter", "ped_commands::CommandGetSeatPedIsTryingToEnter(int)"),
    ("GetVehicleLightsState", "vehicle_commands::CommandGetVehicleLightsState(int, int&, int&)"),
    ("SetVehicleLights", "vehicle_commands::CommandSetVehicleLights(int, int)"),
    ("SetVehicleFullBeam", "vehicle_commands::CommandSetVehicleFullBeam(int, bool)"),
    ("IsHornActive", "audio_commands::CommandIsHornActive(int)"),
    ("SoundCarHorn", "vehicle_commands::CommandSoundCarHorn(int, int, int, bool)"),
    ("IsVehicleSirenOn", "vehicle_commands::CommandIsVehicleSirenOn(int)"),
    ("SetVehicleSiren", "vehicle_commands::CommandSetVehicleSiren(int, bool)"),
    ("GetVehicleDoorAngleRatio", "vehicle_commands::CommandGetVehicleDoorAngleRatio(int, int)"),
    ("SetVehicleDoorOpen", "vehicle_commands::CommandSetVehicleDoorOpen(int, int, bool, bool)"),
    ("SetVehicleDoorShut", "vehicle_commands::CommandSetVehicleDoorShut(int, int, bool)"),
    ("DrawRect", "graphics_commands::CommandDrawRect(float, float, float, float, int, int, int, int, bool)"),
    ("PlaySoundFrontend", "audio_commands::CommandPlaySoundFrontend(int, char const*, char const*, bool)"),
    ("GetInteriorFromEntity", "interior_commands::CommandGetInteriorFromEntity(int)"),
    ("GetPedSourceOfDeath", "ped_commands::CommandGetPedSourceOfDeath(int)"),
    ("IsPedArmed", "weapon_commands::CommandIsPedArmed(int, int)"),
    ("GetWeaponDamage", "weapon_commands::CommandGetWeaponDamage(int, int)"),
    ("GetWeaponDamageType", "weapon_commands::CommandGetWeaponDamageType(int)"),
    ("SetPedToRagdoll", "ped_commands::CommandSetPedToRagdoll(int, int, int, int, bool, bool, bool)"),
    ("ShakeGameplayCam", "camera_commands::CommandShakeGameplayCam(char const*, float)"),
    ("SetPedShootsAtCoord", "ped_commands::CommandSetPedShootsAtCoord(int, rage::scrVector const&, bool)"),
    ("SetPedMaxHealth", "ped_commands::CommandSetPedMaxHealth(int, int)"),
    ("GetSelectedPedWeapon", "weapon_commands::CommandGetSelectedPedWeapon(int)"),
    ("GiveWeaponToPed", "weapon_commands::CommandGiveWeaponToPed(int, int, int, bool, bool)"),
    ("SetCurrentPedWeapon", "weapon_commands::CommandSetCurrentPedWeapon(int, int, bool)"),
    ("SetPedInfiniteAmmoClip", "weapon_commands::CommandSetPedInfiniteAmmoClip(int, bool)"),
    ("GetPedLastWeaponImpactCoord", "weapon_commands::CommandGetPedLastWeaponImpactCoord(int, rage::Vector3&)"),
    ("RequestModel", "streaming_commands::CommandRequestModel(int)"),
    ("HasModelLoaded", "streaming_commands::HasModelLoaded(int)"),
    ("TaskGoStraightToCoord", "task_commands::CommandTaskGoStraightToCoord(int, rage::scrVector const&, float, int, float, float)"),
    ("TaskAimGunAtCoord", "task_commands::CommandTaskAimGunAtCoord(int, rage::scrVector const&, int, bool, bool)"),
    ("TaskEnterVehicle", "task_commands::CommandTaskEnterVehicle(int, int, int, int, float, int, char const*)"),
    ("ClearPedTasksImmediately", "task_commands::CommandClearPedTasksImmediately(int)"),
    ("AddBlipForEntity", "hud_commands::AddBlipForEntity(int)"),
    ("RemoveBlip", "hud_commands::CommandRemoveBlip(int&)"),
    ("ShowHeadingIndicatorOnBlip", "hud_commands::CommandShowHeadingIndicatorOnBlip(int, bool)"),
];

const ACTIVE_THREAD_TLS: u64 = 3088;

const THREAD_NAME: u64 = 428;
const SEND_EVERY: Duration = Duration::from_millis(33);
const LOOK_EVERY: Duration = Duration::from_secs(1);
const POP_EVERY: Duration = Duration::from_millis(100);

const INTERP_DELAY: Duration = Duration::from_millis(80);

const SHARE_NEAR: f32 = 150.0;
const SHARE_FAR: f32 = 220.0;

const BONE_RIGHT_HAND: i32 = 57005;

const TAG_RANGE: f32 = 80.0;

const AVATAR_HEALTH: i32 = 5000;

const SCRATCH: i64 = 2048;
const S_INT: u64 = 160;
const S_INT2: u64 = 168;
const S_V3: u64 = 192;
const S_VEHS: u64 = 256;
const S_PEDS: u64 = 1024;
const S_DICT: u64 = 1360;
const S_CLIP: u64 = 1430;
const S_TEXT: u64 = 1500;
const S_NAME: u64 = 1520;
const NEARBY_MAX: u64 = 40;

struct Peer {
    name: String,
    snaps: VecDeque<(Instant, State)>,
    last: Instant,
    look: Option<Look>,
    vlook: Option<VehLook>,

    pop: Option<(Instant, Vec<PopEntry>)>,

    extras: Option<(Instant, Vec<PopEntry>)>,

    vctl: Option<[u8; 4]>,

    pop_parts: [Option<(Instant, Vec<PopEntry>)>; POP_PARTS],
    clock: Clock,
}

impl Peer {
    fn new(name: String) -> Peer {
        Peer { name, snaps: VecDeque::new(), last: Instant::now(), look: None, vlook: None, pop: None, extras: None, vctl: None, pop_parts: Default::default(), clock: Clock::default() }
    }
}

struct Net {
    // The relay session, shared with the pump thread: it drains remote poses
    // while the game thread publishes ours. The Arcs let both sides reach the
    // one set of peers without borrowing the Net itself.
    session: Arc<Mutex<relay::Session>>,
    peers: Arc<Mutex<HashMap<u8, Peer>>>,

    // Relay peer ids are 16 hex characters while the avatar code is keyed by u8,
    // so each id gets one stable owner number for the life of the session.
    owners: Arc<Mutex<HashMap<String, u8>>>,

    next_owner: Arc<Mutex<u8>>,
    name: String,
    my_id: Mutex<Option<u8>>,
    shots: Mutex<Vec<(u8, Shot)>>,

    kills: Mutex<Vec<(u8, u32)>>,

    taken: Mutex<Vec<(u8, u32)>>,
    explosions: Mutex<Vec<(u8, i32, [f32; 3])>>,

    hits: Mutex<Vec<(u8, u8, u16, u32)>>,

    car_hits: Mutex<Vec<(u8, i16, i16, u8, i16, bool)>>,

    time: Mutex<Option<(u8, [u8; 4], Option<(u8, u8, f32)>)>>,

    deaths: Mutex<Vec<(u8, u8, u32)>>,
}

// The relay protocol has one frame, `pose`. Shots, kills, damage and population
// have nothing to travel in, so the old reliable path is a no-op for now and
// its call sites go away with it in the cleanup pass.
// ponytail: delete this and every caller; nothing may reach the network here.
fn send_reliable(_net: &Net, _kind: u8, _payload: &[u8], _to: Option<u8>) {}

static NET: OnceLock<Option<Net>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

pub fn shutdown() {
    if let Some(Some(net)) = NET.get() {
        if let Ok(mut session) = net.session.lock() {
            session.close();
        }
    }
}

static LOCAL_PED: AtomicU64 = AtomicU64::new(0);
static LOCAL_CAR: AtomicU64 = AtomicU64::new(0);
static LAST_EXPLOSION: Mutex<Option<Instant>> = Mutex::new(None);

pub fn explosion(args: u64) {
    let Some(Some(net)) = NET.get() else { return };
    let mem = &crate::RT.get().unwrap().mem;
    let owner = read_u64(mem, args + 40).unwrap_or(0);
    let mine = owner != 0 && (owner == LOCAL_PED.load(Ordering::Relaxed) || owner == LOCAL_CAR.load(Ordering::Relaxed));
    if !mine {
        return;
    }

    let mut last = LAST_EXPLOSION.lock().unwrap();
    if last.map(|t| t.elapsed() < Duration::from_millis(50)).unwrap_or(false) {
        return;
    }
    *last = Some(Instant::now());
    let kind = read_u32(mem, args).unwrap_or(0);
    let mut payload = kind.to_le_bytes().to_vec();
    for o in [16, 20, 24] {
        payload.extend(read_u32(mem, args + o).unwrap_or(0).to_le_bytes());
    }
    send_reliable(net, proto::EXPLODE, &payload, None);
}

static OTHERS_HERE: AtomicBool = AtomicBool::new(false);

pub fn time_warp(dir: i64) -> i64 {
    if OTHERS_HERE.load(Ordering::Relaxed) { 0 } else { dir }
}

static WANT_ENTER: AtomicBool = AtomicBool::new(false);
static WANT_BULLETS: AtomicBool = AtomicBool::new(false);
static TYPED: Mutex<String> = Mutex::new(String::new());

pub fn key_pressed(code: &str) {
    if code == "KeyH" {
        WANT_ENTER.store(true, Ordering::Relaxed);
    }
    if let Some(c) = code.strip_prefix("Key").filter(|c| c.len() == 1) {
        let mut t = TYPED.lock().unwrap();
        t.push_str(&c.to_ascii_lowercase());
        if t.len() > 32 {
            let cut = t.len() - 32;
            t.drain(..cut);
        }

        if t.ends_with("ineedmorebullets") {
            t.clear();
            WANT_BULLETS.store(true, Ordering::Relaxed);
        }
    }
}

/// Publish our pose. The pump thread holds the session lock across its read, so
/// a busy lock simply means this frame's pose is skipped: poses are 20 Hz and
/// the next one is 50 ms away, which is better than stalling the game thread.
fn publish(net: &Net, s: State) {
    let Ok(mut session) = net.session.try_lock() else { return };
    // Heading is the yaw component; the other two rotation axes have no field.
    if let Err(e) = session.send_pose(s.ped_model, s.pos, [0.0, s.heading, 0.0]) {
        eprintln!("[mp] relay pose failed: {e:#}");
    }
}

// A relay pose carries position, rotation and model and nothing else, so every
// other frame the old protocol sent has nowhere to go. The local half of each
// feature still runs; it just stops replicating.
// ponytail: remove these call sites and their features in the cleanup pass.
fn unreplicated(_net: &Net) {}

/// A relay pose has position, rotation and model, so this is all a remote player
/// is: a ped at that spot, facing that way, at full health. Heading is the yaw
/// component of the rotation, in degrees, which is how State already carries it.
fn state_from_pose(pose: relay::Pose) -> State {
    State { ped_model: pose.model, pos: pose.position, heading: pose.rotation[1], health: 100, ..State::default() }
}

fn net() -> Option<&'static Net> {
    let slot = NET.get_or_init(|| {
        let url = std::env::var("MUGUET_RELAY_URL").ok().filter(|u| relay::valid_url(u))?;
        let room = std::env::var("MUGUET_RELAY_ROOM").ok().filter(|r| relay::valid_room(r))?;
        let key = std::env::var("MUGUET_RELAY_KEY").unwrap_or_default();
        if !relay::valid_key(&key) {
            eprintln!("[mp] the relay access code must be 4-128 printable characters");
            return None;
        }
        // The relay pins a room to one build, so both peers must claim the same.
        let build = std::env::var("MUGUET_BUILD").unwrap_or_else(|_| "android".into());
        let (session, others) = match relay::Session::join(&url, &room, &key, &build) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("[mp] relay join failed: {e:#}");
                return None;
            }
        };
        let name = std::env::var("MUGUET_NAME").ok().filter(|n| !n.is_empty()).unwrap_or_else(|| "Player".into());
        let session = Arc::new(Mutex::new(session));
        let peers: Arc<Mutex<HashMap<u8, Peer>>> = Default::default();
        let owners: Arc<Mutex<HashMap<String, u8>>> = Default::default();
        let next_owner: Arc<Mutex<u8>> = Default::default();
        let net = Net {
            session,
            peers: peers.clone(),
            owners: owners.clone(),
            next_owner: next_owner.clone(),
            session: session.clone(),
            name,
            my_id: Mutex::new(Some(0)),
            shots: Mutex::new(Vec::new()),
            kills: Mutex::new(Vec::new()),
            taken: Mutex::new(Vec::new()),
            explosions: Mutex::new(Vec::new()),
            hits: Mutex::new(Vec::new()),
            car_hits: Mutex::new(Vec::new()),
            time: Mutex::new(None),
            deaths: Mutex::new(Vec::new()),
        };
        eprintln!("[mp] relay {url} room {room}, {} player(s) already there", others.len());
        for id in others {
            // Peers arrive before their first pose, so the room looks occupied
            // from the first frame instead of popping in mid-view.
            let owner = owner_for(&owners, &next_owner, &id);
            peers.lock().unwrap().insert(owner, Peer::new(short_id(&id)));
        }
        std::thread::Builder::new()
            .name("mp-relay".into())
            .spawn(move || relay_pump(session, peers, owners, next_owner))
            .ok();
        Some(net)
    });
    slot.as_ref()
}

/// Relay ids are 16 hex characters and the avatar code is keyed by u8, so each id
/// keeps one owner number for the whole session. Owner 0 is reserved for us.
fn owner_for(owners: &Mutex<HashMap<String, u8>>, next: &Mutex<u8>, id: &str) -> u8 {
    let mut owners = owners.lock().unwrap();
    if let Some(owner) = owners.get(id) {
        return *owner;
    }
    let mut next = next.lock().unwrap();
    *next = next.wrapping_add(1).max(1);
    owners.insert(id.to_string(), *next);
    *next
}

fn short_id(id: &str) -> String {
    id.chars().take(4).collect()
}

/// The relay has one frame, `pose`, so this is the whole remote half of
/// multiplayer: turn each pose into the State the avatar code already speaks.
fn relay_pump(session: Arc<Mutex<relay::Session>>, peers: Arc<Mutex<HashMap<u8, Peer>>>, owners: Arc<Mutex<HashMap<String, u8>>>, next_owner: Arc<Mutex<u8>>) {
    loop {
        let event = match session.lock() {
            Ok(mut session) => session.poll(),
            Err(_) => return,
        };
        match event {
            Some(relay::Event::Pose { id, pose }) => {
                let owner = owner_for(&owners, &next_owner, &id);
                let mut peers = peers.lock().unwrap();
                let peer = peers.entry(owner).or_insert_with(|| Peer::new(short_id(&id)));
                peer.last = Instant::now();
                peer.snaps.push_back((Instant::now(), state_from_pose(pose)));
                while peer.snaps.len() > 32 {
                    peer.snaps.pop_front();
                }
            }
            Some(relay::Event::Left(id)) => {
                if let Some(owner) = owners.lock().unwrap().remove(&id) {
                    if peers.lock().unwrap().remove(&owner).is_some() {
                        eprintln!("[mp] player {} left", short_id(&id));
                    }
                }
            }
            Some(relay::Event::Joined(id)) => eprintln!("[mp] player {} joined", short_id(&id)),
            Some(relay::Event::Failed(why)) => eprintln!("[mp] relay: {why}"),
            Some(_) | None => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Clock {
    gaps: VecDeque<f64>,

    offset: f64,

    delay: f64,
}

impl Clock {
    fn place(&mut self, sent_ms: u32, now: Instant) -> Instant {
        let start = *START.get_or_init(Instant::now);
        let local = now.duration_since(start).as_secs_f64() * 1000.0;
        let gap = local - sent_ms as f64;
        self.gaps.push_back(gap);
        while self.gaps.len() > 90 {
            self.gaps.pop_front();
        }
        self.offset = self.gaps.iter().copied().fold(f64::MAX, f64::min);

        let mut late: Vec<f64> = self.gaps.iter().map(|g| g - self.offset).collect();
        late.sort_by(|a, b| a.total_cmp(b));
        let p90 = late[(late.len() * 9 / 10).min(late.len() - 1)];
        let want = (p90 + SEND_EVERY.as_secs_f64() * 1000.0 + 12.0).clamp(45.0, 300.0);

        self.delay = if want > self.delay { want } else { self.delay + (want - self.delay) * 0.02 };
        let at_ms = (sent_ms as f64 + self.offset).max(0.0);
        start + Duration::from_secs_f64(at_ms / 1000.0)
    }
    fn delay(&self) -> Duration {
        Duration::from_secs_f64(if self.delay > 0.0 { self.delay } else { INTERP_DELAY.as_secs_f64() * 1000.0 } / 1000.0)
    }
}

fn lerp_angle(x: f32, y: f32, k: f32) -> f32 {
    let d = (y - x + 540.0).rem_euclid(360.0) - 180.0;
    x + d * k
}

fn sample(peer: &Peer, at: Instant) -> Option<State> {
    let snaps = &peer.snaps;
    let (t_last, last) = *snaps.back()?;
    if at >= t_last {
        let dt = (at - t_last).as_secs_f32().min(0.25);
        let mut s = last;
        for i in 0..3 {
            s.pos[i] += s.vel[i] * dt;
            s.veh_pos[i] += s.veh_vel[i] * dt;
        }
        return Some(s);
    }
    let i = snaps.iter().position(|(t, _)| *t > at)?;
    if i == 0 {
        return Some(snaps[0].1);
    }
    let ((t0, a), (t1, b)) = (snaps[i - 1], snaps[i]);
    if a.in_vehicle() != b.in_vehicle() || a.veh_model != b.veh_model {
        return Some(b);
    }
    let k = ((at - t0).as_secs_f32() / (t1 - t0).as_secs_f32().max(1e-3)).clamp(0.0, 1.0);
    let lerp = |x: [f32; 3], y: [f32; 3]| [x[0] + (y[0] - x[0]) * k, x[1] + (y[1] - x[1]) * k, x[2] + (y[2] - x[2]) * k];
    let mut s = b;
    s.pos = lerp(a.pos, b.pos);
    s.vel = lerp(a.vel, b.vel);
    s.heading = lerp_angle(a.heading, b.heading, k);
    s.veh_pos = lerp(a.veh_pos, b.veh_pos);
    s.veh_vel = lerp(a.veh_vel, b.veh_vel);
    s.veh_rot = [lerp_angle(a.veh_rot[0], b.veh_rot[0], k), lerp_angle(a.veh_rot[1], b.veh_rot[1], k), lerp_angle(a.veh_rot[2], b.veh_rot[2], k)];
    Some(s)
}

#[derive(Default, Clone, Copy)]
struct Walker {
    target: [f32; 3],

    blend: f32,
    since: Option<Instant>,

    aim: Option<([f32; 3], Instant)>,
}

#[derive(Default)]
struct Avatar {
    ped: i32,
    ped_model: u32,
    veh: i32,
    veh_model: u32,
    veh_colours: [u8; 2],

    veh_damage: (i16, i16, i16, u8, u8, bool),
    veh_look: Option<VehLook>,

    veh_wait: Option<Instant>,

    hit: u16,

    bullet: u16,

    car_hit: (i16, i16, u8, i16, bool),
    blip: i32,
    walk: Walker,
    weapon: u32,
    dead: bool,
    look: Option<Look>,
    aim_until: Option<Instant>,

    veh_spin: f32,

    ragdoll: Option<Instant>,

    entering: Option<(i32, Instant)>,

    leaving_until: Option<Instant>,

    vctl: Option<[u8; 4]>,
    horn_at: Option<Instant>,
}

struct Mirror {
    is_vehicle: bool,

    dead: bool,
    ent: i32,
    driver: i32,
    walk: Walker,
    seen: Instant,

    still: bool,
}

struct Game {
    host: u64,
    host_name: String,
    host_seen: Instant,
    frame_start: Option<Instant>,
    last_send: Instant,
    last_look: Instant,
    sent_look: Option<(Instant, Look)>,
    last_shot: Instant,

    last_impact: [f32; 3],

    last_swing: Instant,

    my_car: i32,

    was_dead: bool,
    last_tidy: Instant,
    last_pop: Instant,
    sent_vlook: Option<(Instant, VehLook)>,
    sent_vctl: Option<(Instant, [u8; 4])>,

    marker: Option<(Instant, bool)>,

    hurt: Vec<(u8, Instant)>,
    last_vlook: Instant,
    last_time: Instant,

    weather: Option<u8>,

    clock_seen: Option<i32>,

    taken: std::collections::HashSet<(u8, u32)>,
    seq: u32,
    avatars: HashMap<u8, Avatar>,

    mirrors: HashMap<(u8, u32), Mirror>,

    pop_owner: Option<u8>,

    met: bool,
    unfreeze_at: Option<Instant>,
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);

const PREFERRED_SCRIPTS: &[&str] = &["main_persistent", "standard_global_reg", "main", "sandbox", "freemode"];

pub fn script_tick(caller: &mut Caller<'_, HostState>, thread: u64, tls: u64) -> anyhow::Result<()> {
    let net = net();
    let mem = &crate::RT.get().unwrap().mem;
    let name = read_cstr(mem, thread + THREAD_NAME);
    let mut guard = GAME.lock().unwrap();
    let game = guard.get_or_insert_with(|| Game {
        host: 0,
        host_name: String::new(),
        host_seen: Instant::now(),
        frame_start: None,
        last_send: Instant::now(),
        last_look: Instant::now(),
        sent_look: None,
        last_shot: Instant::now(),
        last_impact: [0.0; 3],
        last_swing: Instant::now(),
        my_car: 0,
        was_dead: false,
        last_tidy: Instant::now(),
        last_pop: Instant::now(),
        sent_vlook: None,
        sent_vctl: None,
        marker: None,
        hurt: Vec::new(),
        last_vlook: Instant::now(),
        last_time: Instant::now(),
        weather: None,
        clock_seen: None,
        taken: std::collections::HashSet::new(),
        seq: 0,
        avatars: HashMap::new(),
        mirrors: HashMap::new(),
        pop_owner: None,
        met: false,
        unfreeze_at: None,
    });

    let is_host = thread == game.host && name == game.host_name;
    if !is_host {
        let current_rank = PREFERRED_SCRIPTS.iter().position(|s| *s == game.host_name).unwrap_or(usize::MAX);
        let rank = PREFERRED_SCRIPTS.iter().position(|s| *s == name).unwrap_or(usize::MAX);
        let host_gone = game.host_seen.elapsed() > Duration::from_secs(2);
        let started = game.frame_start.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(5);
        if (rank < current_rank && rank != usize::MAX) || (host_gone && (rank != usize::MAX || started)) {
            game.host = thread;
            game.host_name = name.clone();

            game.avatars.clear();
            game.mirrors.clear();
        } else {
            return Ok(());
        }
    }
    game.host_seen = Instant::now();

    let active = tls + ACTIVE_THREAD_TLS;
    let saved = read_u64(mem, active).unwrap_or(0);
    write_u64(mem, active, thread);
    let sp = call(caller, "emscripten_stack_get_current", &[]).map(|v| v.unwrap_i64()).unwrap_or(0);
    let scratch = call(caller, "_emscripten_stack_alloc", &[Val::I64(SCRATCH)]).map(|v| v.unwrap_i64() as u64).unwrap_or(0);
    if scratch != 0 {
        let mut c = Cmd { caller, scratch };
        if WANT_BULLETS.swap(false, Ordering::Relaxed) {
            more_bullets(&mut c);
        }

        if crate::pad::ACTIVE.load(Ordering::Relaxed) || cfg!(any(target_os = "ios", target_os = "android")) {
            let ped = c.i("PlayerPedId", &[]);
            let in_car = ped != 0 && c.exists(ped) && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) != 0;
            crate::pad::IN_VEHICLE.store(in_car, Ordering::Relaxed);

            static HORN_AT: Mutex<Option<Instant>> = Mutex::new(None);
            let mut horn_at = HORN_AT.lock().unwrap();
            if in_car && crate::pad::horn_held() && horn_at.map(|t| t.elapsed() > Duration::from_millis(250)).unwrap_or(true) {
                let veh = c.i("GetVehiclePedIsIn", &[i(ped), b(false)]);
                c.i("SoundCarHorn", &[i(veh), i(350), i(proto::joaat("HELDDOWN") as i32), b(false)]);
                *horn_at = Some(Instant::now());
            }
        }
        if let Some(net) = net {
            frame(&mut c, game, net);
        }
    }
    if sp != 0 {
        let _ = call(caller, "_emscripten_stack_restore", &[Val::I64(sp)]);
    }
    write_u64(mem, active, saved);
    Ok(())
}

fn call(caller: &mut Caller<'_, HostState>, name: &str, args: &[Val]) -> Option<Val> {
    let f = caller.get_export(name)?.into_func()?;
    let ty = f.ty(&mut *caller);
    let args = crate::coerce_args(args, &ty);
    let mut out: Vec<Val> = ty.results().map(|t| crate::default_val(&t)).collect();
    if let Err(e) = f.call(&mut *caller, &args, &mut out) {
        eprintln!("[mp] {name} trapped: {e}");
        return None;
    }
    Some(out.into_iter().next().unwrap_or(Val::I32(0)))
}

struct Cmd<'a, 'b> {
    caller: &'a mut Caller<'b, HostState>,
    scratch: u64,
}

impl Cmd<'_, '_> {
    fn i(&mut self, cmd: &str, args: &[Val]) -> i32 {
        match call(self.caller, &format!("mp:{cmd}"), args) {
            Some(Val::I32(v)) => v,
            Some(Val::I64(v)) => v as i32,
            _ => 0,
        }
    }
    fn f(&mut self, cmd: &str, args: &[Val]) -> f32 {
        match call(self.caller, &format!("mp:{cmd}"), args) {
            Some(Val::F32(v)) => f32::from_bits(v),
            _ => 0.0,
        }
    }
    fn mem(&self) -> &'static wasmtime::SharedMemory {
        &crate::RT.get().unwrap().mem
    }
    fn read_i32(&self, off: u64) -> i32 {
        read_u32(self.mem(), self.scratch + off).unwrap_or(0) as i32
    }
    fn read_f32(&self, off: u64) -> f32 {
        f32::from_bits(read_u32(self.mem(), self.scratch + off).unwrap_or(0))
    }

    fn v(&mut self, cmd: &str, args: &[Val]) -> [f32; 3] {
        let mut full = vec![Val::I64(self.scratch as i64)];
        full.extend_from_slice(args);
        call(self.caller, &format!("mp:{cmd}"), &full);
        [self.read_f32(0), self.read_f32(8), self.read_f32(16)]
    }

    fn v_packed(&mut self, cmd: &str, args: &[Val]) -> [f32; 3] {
        let mut full = vec![Val::I64(self.scratch as i64)];
        full.extend_from_slice(args);
        call(self.caller, &format!("mp:{cmd}"), &full);
        [self.read_f32(0), self.read_f32(4), self.read_f32(8)]
    }

    fn vec_arg(&self, slot: u64, v: [f32; 3]) -> Val {
        let p = self.scratch + 32 * slot;
        let m = self.mem();
        for (i, c) in v.iter().enumerate() {
            crate::write_u32(m, p + 8 * i as u64, c.to_bits());
            crate::write_u32(m, p + 8 * i as u64 + 4, 0);
        }
        Val::I64(p as i64)
    }

    fn ref_arg(&self, v: i32) -> Val {
        crate::write_u32(self.mem(), self.scratch + S_INT, v as u32);
        Val::I64((self.scratch + S_INT) as i64)
    }

    fn c_str(&self, off: u64, s: &str) {
        let mut b: Vec<u8> = s.bytes().take(60).collect();
        b.push(0);
        crate::write_bytes(self.mem(), self.scratch + off, &b);
    }
    fn ptr(&self, off: u64) -> Val {
        Val::I64((self.scratch + off) as i64)
    }
    fn exists(&mut self, ent: i32) -> bool {
        ent != 0 && self.i("DoesEntityExist", &[i(ent)]) != 0
    }

    fn nearby(&mut self, cmd: &str, ped: i32, off: u64, extra: &[Val]) -> Vec<i32> {
        let m = self.mem();
        write_u64(m, self.scratch + off, NEARBY_MAX);
        let mut args = vec![i(ped), self.ptr(off)];
        args.extend_from_slice(extra);
        let n = (self.i(cmd, &args).max(0) as u64).min(NEARBY_MAX);
        (0..n).map(|k| read_u32(m, self.scratch + off + 8 + 8 * k).unwrap_or(0) as i32).filter(|e| *e != 0).collect()
    }
}

fn b(v: bool) -> Val {
    Val::I32(v as i32)
}
fn f(v: f32) -> Val {
    Val::F32(v.to_bits())
}
fn i(v: i32) -> Val {
    Val::I32(v)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn weapon(name: &str) -> u32 {
    proto::joaat(name)
}

fn weapon_name(hash: u32) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("weapon_unarmed", "fists"), ("weapon_knife", "knife"), ("weapon_bat", "bat"), ("weapon_crowbar", "crowbar"),
        ("weapon_golfclub", "golf club"), ("weapon_hammer", "hammer"), ("weapon_nightstick", "nightstick"),
        ("weapon_pistol", "pistol"), ("weapon_combatpistol", "combat pistol"), ("weapon_appistol", "AP pistol"),
        ("weapon_pistol50", "pistol .50"), ("weapon_stungun", "taser"), ("weapon_microsmg", "micro SMG"), ("weapon_smg", "SMG"),
        ("weapon_assaultsmg", "assault SMG"), ("weapon_assaultrifle", "assault rifle"), ("weapon_carbinerifle", "carbine rifle"),
        ("weapon_advancedrifle", "advanced rifle"), ("weapon_mg", "MG"), ("weapon_combatmg", "combat MG"),
        ("weapon_pumpshotgun", "pump shotgun"), ("weapon_sawnoffshotgun", "sawed-off"), ("weapon_assaultshotgun", "assault shotgun"),
        ("weapon_bullpupshotgun", "bullpup shotgun"), ("weapon_sniperrifle", "sniper rifle"), ("weapon_heavysniper", "heavy sniper"),
        ("weapon_grenadelauncher", "grenade launcher"), ("weapon_rpg", "RPG"), ("weapon_minigun", "minigun"),
        ("weapon_grenade", "grenade"), ("weapon_stickybomb", "sticky bomb"), ("weapon_molotov", "molotov"),
        ("weapon_run_over_by_car", "run over"), ("weapon_rammed_by_car", "rammed"), ("weapon_explosion", "explosion"),
        ("vehicle_weapon_tank", "tank"), ("weapon_fall", "fall"), ("weapon_fire", "fire"),
    ];
    NAMES.iter().find(|(n, _)| proto::joaat(n) == hash).map(|(_, s)| *s)
}

fn more_bullets(c: &mut Cmd) {
    const ALL: &[&str] = &[
        "weapon_knife", "weapon_bat", "weapon_pistol", "weapon_combatpistol", "weapon_appistol", "weapon_pistol50",
        "weapon_microsmg", "weapon_smg", "weapon_assaultsmg", "weapon_assaultrifle", "weapon_carbinerifle",
        "weapon_advancedrifle", "weapon_mg", "weapon_combatmg", "weapon_pumpshotgun", "weapon_sawnoffshotgun",
        "weapon_assaultshotgun", "weapon_sniperrifle", "weapon_heavysniper", "weapon_grenadelauncher",
        "weapon_rpg", "weapon_minigun", "weapon_grenade", "weapon_stickybomb", "weapon_molotov",
    ];
    let ped = c.i("PlayerPedId", &[]);
    if ped == 0 {
        return;
    }
    for w in ALL {
        c.i("GiveWeaponToPed", &[i(ped), i(weapon(w) as i32), i(9999), b(false), b(false)]);
    }
    c.i("SetPedInfiniteAmmoClip", &[i(ped), b(true)]);
    c.i("SetCurrentPedWeapon", &[i(ped), i(weapon("weapon_carbinerifle") as i32), b(true)]);
    eprintln!("[mp] more bullets");
    play_sound("morebullets.mp3");
}

fn play_sound(name: &str) {
    if std::env::var("MUGUET_MUTE").map(|v| v == "1").unwrap_or(false) {
        return;
    }
    let mut places: Vec<std::path::PathBuf> = crate::config::bundled_resources().into_iter().map(|r| r.join(name)).collect();
    if let Ok(exe) = std::env::current_exe() {
        places.extend(exe.ancestors().map(|d| d.join("assets").join(name)));
    }
    if let Some(path) = places.into_iter().find(|p| p.exists()) {
        let _ = std::process::Command::new("/usr/bin/afplay").arg(path).spawn();
    }
}

fn frame(c: &mut Cmd, game: &mut Game, net: &Net) {
    let my_id = *net.my_id.lock().unwrap();
    let ped = c.i("PlayerPedId", &[]);
    let me_ok = ped != 0 && c.exists(ped);
    let my_pos = if me_ok { c.v("GetEntityCoords", &[i(ped), b(false)]) } else { [0.0; 3] };
    let ptr_of = |c: &mut Cmd, e: i32| match call(c.caller, "mp:GetBaseFromGuid", &[i(e)]) { Some(Val::I64(p)) => p as u64, _ => 0 };
    LOCAL_PED.store(if me_ok { ptr_of(c, ped) } else { 0 }, Ordering::Relaxed);
    let my_car = if me_ok && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) != 0 { c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) } else { 0 };
    LOCAL_CAR.store(if my_car != 0 { ptr_of(c, my_car) } else { 0 }, Ordering::Relaxed);

    if me_ok && game.my_car != my_car {
        let left = game.my_car;
        if left != 0 && c.exists(left) && c.i("IsEntityDead", &[i(left), b(false)]) == 0 && !game.avatars.values().any(|a| a.veh == left) && !game.mirrors.values().any(|m| m.ent == left) {
            park(left, c.i("GetEntityModel", &[i(left)]) as u32, 0);
        }
        game.my_car = my_car;
    }

    if me_ok && my_id.is_some() {
        if game.last_send.elapsed() >= SEND_EVERY {
            game.last_send = Instant::now();
            let s = local_state(c, game, ped, my_pos);
            publish(net, s);
        }
        let veh = if c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) != 0 { c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) } else { 0 };
        let own_car = veh != 0 && !game.avatars.values().any(|a| a.veh == veh);
        if own_car && game.last_vlook.elapsed() >= LOOK_EVERY {
            game.last_vlook = Instant::now();
            let look = read_vlook(c, veh);
            let resend = game.sent_vlook.map(|(t, l)| l != look || t.elapsed() > Duration::from_secs(4)).unwrap_or(true);
            if resend {
                let mut payload = Vec::new();
                look.encode(&mut payload);
            unreplicated(net);
                game.sent_vlook = Some((Instant::now(), look));
            }
        }

        let driver = veh != 0 && c.i("GetPedInVehicleSeat", &[i(veh), i(-1), b(false)]) == ped;
        if own_car && driver {
            let ctl = read_vctl(c, veh);
            let resend = game.sent_vctl.map(|(t, v)| v != ctl || t.elapsed() > Duration::from_secs(1)).unwrap_or(true);
            if resend {
            unreplicated(net);
                game.sent_vctl = Some((Instant::now(), ctl));
            }
        }

        if veh != 0 {
            if let Some(key) = game.mirrors.iter().find(|(_, m)| m.is_vehicle && m.ent == veh).map(|(k, _)| *k) {
                game.mirrors.remove(&key);
                game.taken.insert(key);
                let mut payload = vec![key.0 & !EXTRAS];
                payload.extend(key.1.to_le_bytes());
                send_reliable(net, proto::TAKEN, &payload, Some(key.0 & !EXTRAS));
                eprintln!("[mp] took player {}'s traffic car", key.0 & !EXTRAS);
            }
        }
        if game.last_look.elapsed() >= LOOK_EVERY {
            game.last_look = Instant::now();
            let look = read_look(c, ped);
            let resend = game.sent_look.map(|(t, l)| l != look || t.elapsed() > Duration::from_secs(4)).unwrap_or(true);
            if resend {
                let mut payload = Vec::new();
                look.encode(&mut payload);
            unreplicated(net);
                game.sent_look = Some((Instant::now(), look));
            }
        }

        const ARMED_GUN: i32 = 4;
        const ARMED_THROWN: i32 = 2;
        let gun = c.i("IsPedArmed", &[i(ped), i(ARMED_GUN)]) != 0;

        const INPUT_ATTACK: i32 = 24;
        const INPUT_MELEE_LIGHT: i32 = 140;
        const INPUT_MELEE_HEAVY: i32 = 141;
        let melee = !gun && c.i("IsPedArmed", &[i(ped), i(ARMED_THROWN)]) == 0 && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) == 0;
        if melee && game.last_swing.elapsed() >= Duration::from_millis(450)
            && (c.i("IsPedShooting", &[i(ped)]) != 0 || [INPUT_ATTACK, INPUT_MELEE_LIGHT, INPUT_MELEE_HEAVY].iter().any(|k| c.i("IsControlPressed", &[i(0), i(*k)]) != 0))
        {
            game.last_swing = Instant::now();
            let target = c.i("GetMeleeTargetForPed", &[i(ped)]);
            if let Some((id, _)) = game.avatars.iter().find(|(_, a)| a.ped != 0 && a.ped == target) {
                eprintln!("[mp] swinging at player {id}");
            }
            let to = if target != 0 && c.exists(target) {
                c.v("GetEntityCoords", &[i(target), b(false)])
            } else {
                let h = c.f("GetEntityHeading", &[i(ped)]).to_radians();
                [my_pos[0] - 1.2 * h.sin(), my_pos[1] + 1.2 * h.cos(), my_pos[2]]
            };
            let shot = Shot { weapon: c.i("GetSelectedPedWeapon", &[i(ped)]) as u32, from: my_pos, to };
            let mut payload = Vec::new();
            shot.encode(&mut payload);
            unreplicated(net);
        }
        if game.last_shot.elapsed() >= Duration::from_millis(45) && c.i("IsPedShooting", &[i(ped)]) != 0 && gun {
            game.last_shot = Instant::now();
            let shot = local_shot(c, ped, my_pos);
            if shot.to != game.last_impact {
                game.last_impact = shot.to;
                bullet_on_driver(c, game, shot.weapon, shot.to);
            }
            let mut payload = Vec::new();
            shot.encode(&mut payload);
            unreplicated(net);
        }
    }

    if let Some(t) = game.unfreeze_at {
        if Instant::now() >= t {
            c.i("FreezeEntityPosition", &[i(ped), b(false)]);
            game.unfreeze_at = None;
        }
    }
    if !game.met && game.seq > 100 && me_ok {
        if let Some(me) = my_id {
            let first = {
                let peers = net.peers.lock().unwrap();
                peers.iter().filter(|(id, p)| **id < me && !p.snaps.is_empty()).min_by_key(|(id, _)| **id).map(|(id, p)| (*id, p.snaps.back().unwrap().1))
            };
            if let Some((id, st)) = first {
                if c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) == 0 {
                    let at = if st.in_vehicle() { st.veh_pos } else { st.pos };
                    let pos = c.vec_arg(1, [at[0] + 2.5, at[1] + 2.5, at[2] + 0.5]);
                    c.i("SetEntityCoordsNoOffset", &[i(ped), pos, b(false), b(false), b(false)]);

                    c.i("FreezeEntityPosition", &[i(ped), b(true)]);
                    game.unfreeze_at = Some(Instant::now() + Duration::from_secs(3));
                    eprintln!("[mp] moved next to player {id} at {at:.0?}");
                }
                game.met = true;
            }
        }
    }

    let now = Instant::now();

    let mut spins: HashMap<u8, f32> = HashMap::new();
    let vctls: HashMap<u8, [u8; 4]> = net.peers.lock().unwrap().iter().filter_map(|(id, p)| p.vctl.map(|v| (*id, v))).collect();
    let states: Vec<(u8, Option<State>, Option<Look>, Option<VehLook>)> = {
        let mut peers = net.peers.lock().unwrap();
        peers.retain(|_, p| p.last.elapsed() < Duration::from_secs(proto::TIMEOUT_SECS + 2));
        peers.iter().map(|(id, p)| {
            let at = now - p.clock.delay();
            let s = sample(p, at);
            if let (Some(a), Some(b)) = (sample(p, at - Duration::from_millis(60)), s) {
                if a.in_vehicle() && b.in_vehicle() {
                    spins.insert(*id, ((b.veh_rot[2] - a.veh_rot[2] + 540.0).rem_euclid(360.0) - 180.0) / 0.06);
                }
            }
            (*id, s, p.look, p.vlook)
        }).collect()
    };
    let gone: Vec<u8> = game.avatars.keys().filter(|id| !states.iter().any(|(s, _, _, _)| s == *id)).copied().collect();
    for id in gone {
        if let Some(a) = game.avatars.remove(&id) {
            despawn(c, a);
        }
    }

    let mut cars: HashMap<u8, i32> = game.avatars.iter().filter(|(_, a)| a.veh != 0).map(|(id, a)| (*id, a.veh)).collect();
    if let (Some(me), true) = (my_id, me_ok) {
        let v = c.i("GetVehiclePedIsIn", &[i(ped), b(false)]);
        if v != 0 {
            cars.insert(me, v);
        }
    }

    let mut order: Vec<&(u8, Option<State>, Option<Look>, Option<VehLook>)> = states.iter().collect();
    order.sort_by_key(|(_, s, _, _)| s.map(|s| s.veh_owner != 0).unwrap_or(false));
    for (id, s, look, vlook) in order.iter().copied() {
        let Some(s) = s else { continue };
        let a = game.avatars.entry(*id).or_default();
        a.veh_spin = spins.get(id).copied().unwrap_or(0.0);
        update_avatar(c, *id, a, s, *look, &cars);

        if let (Some(l), true) = (vlook, a.veh != 0 && me_ok) {
            if l.radio != 255 && c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) == a.veh && c.i("GetPlayerRadioStationIndex", &[]) != l.radio as i32 {
                c.i("RetuneRadioToStationIndex", &[i(l.radio as i32)]);
            }
        }
        if let (Some(l), true) = (vlook, a.veh != 0) {
            if l.model == a.veh_model && a.veh_look != Some(*l) {
                apply_vlook(c, a.veh, l);
                a.veh_look = Some(*l);
            }
        }
        if let (Some(v), true) = (vctls.get(id), a.veh != 0 && s.in_vehicle() && s.veh_owner == 0) {
            if a.vctl != Some(*v) || v[1] != 0 {
                apply_vctl(c, a, *v);
            }
        }
        if a.veh != 0 {
            cars.insert(*id, a.veh);
        }
    }

    *AVATAR_ENTS.lock().unwrap() = game.avatars.values().flat_map(|a| [a.ped, a.veh, a.entering.map(|e| e.0).unwrap_or(0)]).filter(|e| *e != 0).collect();

    if game.last_tidy.elapsed() >= Duration::from_secs(1) {
        game.last_tidy = Instant::now();
        let where_: HashMap<u8, [f32; 3]> = states.iter().filter_map(|(id, s, ..)| s.map(|s| (*id, if s.in_vehicle() { s.veh_pos } else { s.pos }))).collect();
        tidy_parked(c, &where_);
    }

    if me_ok {
        let dead = c.i("IsPedDeadOrDying", &[i(ped), b(true)]) != 0;
        if dead && !game.was_dead {
            let by = c.i("GetPedSourceOfDeath", &[i(ped)]);
            let killer = game.avatars.iter().find(|(_, a)| by != 0 && (a.ped == by || a.veh == by)).map(|(id, _)| *id).unwrap_or(0);
            let cause = c.i("GetPedCauseOfDeath", &[i(ped)]) as u32;
            let mut payload = vec![killer];
            payload.extend(cause.to_le_bytes());
            send_reliable(net, proto::KILLFEED, &payload, None);
            if let Some(me) = my_id {
                net.deaths.lock().unwrap().push((me, killer, cause));
            }
        }
        game.was_dead = dead;
    }
    let deaths = std::mem::take(&mut *net.deaths.lock().unwrap());
    for (victim, killer, cause) in deaths {
        let name = |id: u8| if Some(id) == my_id { net.name.clone() } else { net.peers.lock().unwrap().get(&id).map(|p| p.name.clone()).unwrap_or_else(|| format!("Player {id}")) };
        let text = match (killer, weapon_name(cause)) {
            (0, _) => format!("{} died", name(victim)),
            (k, Some(w)) => format!("{} killed {} ({w})", name(k), name(victim)),
            (k, None) => format!("{} killed {}", name(k), name(victim)),
        };
        eprintln!("[mp] {text}");
        if killer != 0 && Some(killer) == my_id && Some(victim) != my_id {
            game.marker = Some((Instant::now(), true));
            frontend_sound(c, "CHECKPOINT_NORMAL", "HUD_MINI_GAME_SOUNDSET");
        }
        c.c_str(S_TEXT, "STRING");
        c.c_str(S_NAME, &text);
        c.i("BeginTheFeedPost", &[c.ptr(S_TEXT)]);
        c.i("AddTextComponentSubStringPlayerName", &[c.ptr(S_NAME)]);
        c.i("EndTheFeedPostTicker", &[b(false), b(true)]);
    }

    if me_ok {
        let my_weapon = c.i("GetSelectedPedWeapon", &[i(ped)]) as u32;
        let melee = if my_weapon == weapon("weapon_unarmed") || my_weapon == 0 { 20 } else { 30 };
        for a in game.avatars.values_mut() {
            if my_weapon == weapon("weapon_stungun") && a.hit > 0 {
                a.hit = 1;
            }
            let bullet = std::mem::take(&mut a.bullet);
            if a.hit == 0 {
                a.hit = bullet;
            }
            if a.ped == 0 || a.dead || !c.exists(a.ped) {
                continue;
            }
            if c.i("HasEntityBeenDamagedByEntity", &[i(a.ped), i(ped), b(true)]) != 0 {
                if my_weapon == weapon("weapon_stungun") {
                    a.hit = 1;
                } else if a.hit == 0 {
                    a.hit = melee;
                }
                c.i("ClearEntityLastDamageEntity", &[i(a.ped)]);
            }
        }
    }
    let my_weapon = if me_ok { c.i("GetSelectedPedWeapon", &[i(ped)]) as u32 } else { 0 };
    let mut game_marker = game.marker;
    for (id, a) in game.avatars.iter_mut() {
        if a.hit > 0 {
            let mut p = vec![*id];
            p.extend(a.hit.to_le_bytes());
            p.extend(my_weapon.to_le_bytes());
            eprintln!("[mp] hit player {id}: {} damage (weapon {my_weapon:#x})", a.hit);
            if !matches!(game_marker, Some((_, true))) {
                game_marker = Some((Instant::now(), false));
            }
            send_reliable(net, proto::HIT, &p, Some(*id));
            a.hit = 0;
        }
        if a.car_hit != (0, 0, 0, 0, false) {
            let mut p = vec![*id];
            p.extend(a.car_hit.0.to_le_bytes());
            p.extend(a.car_hit.1.to_le_bytes());
            p.push(a.car_hit.2);
            p.extend(a.car_hit.3.to_le_bytes());
            p.push(a.car_hit.4 as u8);
            send_reliable(net, proto::VHIT, &p, Some(*id));
            a.car_hit = (0, 0, 0, 0, false);
        }
    }
    if game_marker.map(|m| m.0) != game.marker.map(|m| m.0) {
        frontend_sound(c, "NAV_UP_DOWN", "HUD_FRONTEND_DEFAULT_SOUNDSET");
        game.marker = game_marker;
    }

    let hits = std::mem::take(&mut *net.hits.lock().unwrap());
    if me_ok {
        for (from, target, dmg, w) in hits {
            if Some(target) != my_id || c.i("IsPedDeadOrDying", &[i(ped), b(true)]) != 0 {
                continue;
            }
            game.hurt.retain(|(id, _)| *id != from);
            game.hurt.push((from, Instant::now()));
            eprintln!("[mp] got hit: {dmg} damage (weapon {w:#x}), health {}", c.i("GetEntityHealth", &[i(ped)]));

            if w == weapon("weapon_stungun") {
                if c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) == 0 {
                    c.i("SetPedToRagdoll", &[i(ped), i(4000), i(5000), i(0), b(false), b(false), b(false)]);
                }
                continue;
            }

            const DAMAGE_MELEE: i32 = 2;
            if w != 0 && c.i("GetWeaponDamageType", &[i(w as i32)]) == DAMAGE_MELEE {
                c.c_str(S_NAME, "SMALL_EXPLOSION_SHAKE");
                c.i("ShakeGameplayCam", &[c.ptr(S_NAME), f(0.25)]);
                if w != weapon("weapon_unarmed") && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) == 0 {
                    c.i("SetPedToRagdoll", &[i(ped), i(600), i(900), i(0), b(false), b(false), b(false)]);
                }
            }
            let mut dmg = dmg as i32;
            let armour = c.i("GetPedArmour", &[i(ped)]);
            let soak = armour.min(dmg);
            if soak > 0 {
                c.i("SetPedArmour", &[i(ped), i(armour - soak)]);
                dmg -= soak;
            }
            if dmg > 0 {
                let h = c.i("GetEntityHealth", &[i(ped)]) - dmg;

                c.i("SetEntityHealth", &[i(ped), i(if h < 100 { 0 } else { h }), i(0)]);
            }
        }
    }
    let car_hits = std::mem::take(&mut *net.car_hits.lock().unwrap());
    if my_car != 0 {
        for (target, de, db, tyres, dk, destroyed) in car_hits {
            if Some(target) != my_id {
                continue;
            }
            if destroyed {
                if c.i("IsEntityDead", &[i(my_car), b(false)]) == 0 {
                    c.i("ExplodeVehicle", &[i(my_car), b(true), b(false)]);
                }
                continue;
            }
            if dk > 0 {
                let t = c.f("GetVehiclePetrolTankHealth", &[i(my_car)]);
                c.i("SetVehiclePetrolTankHealth", &[i(my_car), f(t - dk as f32)]);
            }
            let e = c.f("GetVehicleEngineHealth", &[i(my_car)]);
            c.i("SetVehicleEngineHealth", &[i(my_car), f(e - de as f32)]);
            let bh = c.f("GetVehicleBodyHealth", &[i(my_car)]);
            c.i("SetVehicleBodyHealth", &[i(my_car), f(bh - db as f32)]);
            for w in 0..6 {
                if tyres & (1 << w) != 0 {
                    c.i("SetVehicleTyreBurst", &[i(my_car), i(w), b(false), f(1000.0)]);
                }
            }
        }
    }

    let clock = c.i("GetClockHours", &[]) * 60 + c.i("GetClockMinutes", &[]);
    if let (Some(seen), false) = (game.clock_seen, states.is_empty()) {
        let jump = (clock - seen).rem_euclid(1440);
        if jump > 5 && jump < 1435 {
            c.i("SetClockTime", &[i(seen / 60), i(seen % 60), i(0)]);
            eprintln!("[mp] clock jumped {:02}:{:02} -> {:02}:{:02}: kept the session's", seen / 60, seen % 60, clock / 60, clock % 60);
        } else {
            game.clock_seen = Some(clock);
        }
    } else {
        game.clock_seen = Some(clock);
    }
    if let Some(me) = my_id {
        let lowest = states.iter().map(|(id, ..)| *id).chain(std::iter::once(me)).min().unwrap_or(me);
        if lowest == me && !states.is_empty() && game.last_time.elapsed() >= Duration::from_secs(1) {
            game.last_time = Instant::now();
            let w = c.i("GetNextWeatherTypeHashName", &[]) as u32;
            let wi = proto::WEATHERS.iter().position(|n| proto::joaat(n) == w).unwrap_or(255) as u8;
            let mut t = vec![c.i("GetClockHours", &[]) as u8, c.i("GetClockMinutes", &[]) as u8, c.i("GetClockSeconds", &[]) as u8, wi];

            let (prev, next, mix) = weather_state(c);
            t.extend([prev.clamp(0, 255) as u8, next.clamp(0, 255) as u8]);
            t.extend(mix.to_le_bytes());
            unreplicated(net);
        }
        let follow = net.time.lock().unwrap().take();
        if let Some((from, [h, m, sec, wi], state)) = follow {
            if from == lowest && from != me {
                let now = c.i("GetClockHours", &[]) * 60 + c.i("GetClockMinutes", &[]);
                let want = h as i32 * 60 + m as i32;
                let off = (now - want).rem_euclid(1440).min((want - now).rem_euclid(1440));
                if off > 1 {
                    c.i("SetClockTime", &[i(h as i32), i(m as i32), i(sec as i32)]);
                }
                game.clock_seen = Some(want);
                if let Some((prev, next, mix)) = state {
                    const STATE_SYNC: u8 = 254;
                    if game.weather != Some(STATE_SYNC) {
                        c.i("ClearWeatherTypePersist", &[]);
                        game.weather = Some(STATE_SYNC);
                    }
                    let (p0, n0, m0) = weather_state(c);
                    if p0 != prev as i32 || n0 != next as i32 || (m0 - mix).abs() > 0.03 {
                        c.i("SetWeatherState", &[i(prev as i32), i(next as i32), f(mix)]);
                    }
                } else if let Some(name) = proto::WEATHERS.get(wi as usize) {
                    if game.weather != Some(wi) {
                        c.c_str(S_NAME, name);
                        c.i("SetWeatherTypeNowPersist", &[c.ptr(S_NAME)]);
                        game.weather = Some(wi);
                    }
                }
            }
        }
    }

    OTHERS_HERE.store(!states.is_empty(), Ordering::Relaxed);
    if !states.is_empty() {
        c.i("SetTimeScale", &[f(1.0)]);
    }

    if !game.avatars.is_empty() {
        let _ = punch_anim(c);
    }

    let shots = std::mem::take(&mut *net.shots.lock().unwrap());
    for (id, shot) in shots {
        if let Some(a) = game.avatars.get_mut(&id) {
            fire(c, a, &shot, me_ok.then_some(my_pos));
        }
    }

    let explosions = std::mem::take(&mut *net.explosions.lock().unwrap());
    for (id, kind, pos) in explosions {
        let owner = game.avatars.get(&id).map(|a| a.ped).filter(|p| c.exists(*p));
        let at = c.vec_arg(1, pos);
        match owner {
            Some(p) => c.i("AddOwnedExplosion", &[i(p), at, i(kind), f(1.0), b(true), b(false), f(1.0)]),
            None => c.i("AddExplosion", &[at, i(kind), f(1.0), b(true), b(false), f(1.0), b(false)]),
        };
    }

    let kills = std::mem::take(&mut *net.kills.lock().unwrap());
    for (owner, key) in kills {
        let ent = key as i32;
        if Some(owner) != my_id || !c.exists(ent) || c.i("IsEntityAMissionEntity", &[i(ent)]) != 0 {
            continue;
        }
        if c.i("IsEntityAPed", &[i(ent)]) != 0 {
            c.i("SetEntityHealth", &[i(ent), i(0), i(0)]);
        } else if c.i("IsEntityDead", &[i(ent), b(false)]) == 0 {
            c.i("ExplodeVehicle", &[i(ent), b(true), b(false)]);
        }
    }

    let taken = std::mem::take(&mut *net.taken.lock().unwrap());
    for (owner, key) in taken {
        let veh = key as i32;
        if Some(owner) != my_id || !c.exists(veh) || c.i("IsEntityAMissionEntity", &[i(veh)]) != 0 {
            continue;
        }
        if me_ok && c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) == veh {
            continue;
        }

        if game.avatars.values().any(|a| a.veh == veh) {
            continue;
        }
        let driver = c.i("GetPedInVehicleSeat", &[i(veh), i(-1), b(false)]);
        for e in [driver, veh] {
            if c.exists(e) {
                c.i("SetEntityAsMissionEntity", &[i(e), b(true), b(true)]);
            }
        }
        delete_ped(c, driver);
        delete_vehicle(c, veh);
    }

    if me_ok {
        draw_hud(c, game);
    }

    let names: HashMap<u8, String> = net.peers.lock().unwrap().iter().map(|(id, p)| {
        let tag = match p.snaps.back() {
            Some((t, _)) if t.elapsed() > Duration::from_millis(1500) => format!("{} (paused)", p.name),
            Some((_, s)) => format!("{} - {}%", p.name, ((s.health as i32 - 100).clamp(0, 100))),
            None => p.name.clone(),
        };
        (*id, tag)
    }).collect();
    for (id, a) in &game.avatars {
        if a.dead || !c.exists(a.ped) || c.i("IsEntityOnScreen", &[i(a.ped)]) == 0 {
            continue;
        }
        let at = c.v("GetEntityCoords", &[i(a.ped), b(false)]);
        if dist(at, my_pos) < TAG_RANGE {
            name_tag(c, at, names.get(id).map(String::as_str).unwrap_or("Player"));
        }
    }

    if WANT_ENTER.swap(false, Ordering::Relaxed) && me_ok && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) == 0 {
        let near = game.avatars.values().filter(|a| a.veh != 0).map(|a| a.veh).min_by(|x, y| {
            let dx = dist(c_pos(c, *x), my_pos);
            let dy = dist(c_pos(c, *y), my_pos);
            dx.total_cmp(&dy)
        });
        if let Some(veh) = near.filter(|v| dist(c_pos(c, *v), my_pos) < 10.0) {
            if let Some(seat) = (0..6).find(|s| c.i("IsVehicleSeatFree", &[i(veh), i(*s), b(false)]) != 0) {
                c.i("TaskEnterVehicle", &[i(ped), i(veh), i(10000), i(seat), f(1.0), i(1), Val::I64(0)]);
                eprintln!("[mp] getting in a friend's car, seat {seat}");
            }
        }
    }

    if me_ok {
        population(c, game, net, my_id, ped, my_pos);
    }
}

fn frontend_sound(c: &mut Cmd, name: &str, set: &str) {
    c.c_str(S_DICT, name);
    c.c_str(S_CLIP, set);
    c.i("PlaySoundFrontend", &[i(-1), c.ptr(S_DICT), c.ptr(S_CLIP), b(true)]);
}

fn rect(c: &mut Cmd, x: f32, y: f32, w: f32, h: f32, rgba: [i32; 4]) {
    c.i("DrawRect", &[f(x), f(y), f(w), f(h), i(rgba[0]), i(rgba[1]), i(rgba[2]), i(rgba[3]), b(false)]);
}

fn draw_hud(c: &mut Cmd, game: &mut Game) {
    const ASPECT: f32 = 16.0 / 9.0;
    if let Some((t, kill)) = game.marker {
        let shown = if kill { 600 } else { 220 };
        let age = t.elapsed().as_millis() as i32;
        if age < shown {
            let alpha = 255 - 200 * age / shown;
            let col = if kill { [230, 40, 40, alpha] } else { [255, 255, 255, alpha] };
            let d = 0.0035;
            for k in 2..=5 {
                let o = k as f32 * d;
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    rect(c, 0.5 + sx * o, 0.5 + sy * o * ASPECT, 0.0028, 0.0028 * ASPECT, col);
                }
            }
        } else {
            game.marker = None;
        }
    }
    game.hurt.retain(|(_, t)| t.elapsed() < Duration::from_millis(1400));
    if game.hurt.is_empty() {
        return;
    }
    let ped = c.i("PlayerPedId", &[]);
    let me = c_pos(c, ped);
    let cam_yaw = c.v("GetGameplayCamRot", &[i(2)])[2];
    let hurt = game.hurt.clone();
    for (id, t) in hurt {
        let Some(a) = game.avatars.get(&id) else { continue };
        let ent = if a.veh != 0 { a.veh } else { a.ped };
        if ent == 0 || !c.exists(ent) {
            continue;
        }
        let at = c_pos(c, ent);

        let bearing = (-(at[0] - me[0])).atan2(at[1] - me[1]).to_degrees();
        let rel = (bearing - cam_yaw).to_radians();
        let alpha = (220.0 * (1.0 - t.elapsed().as_secs_f32() / 1.4)).max(0.0) as i32;
        for k in -3..=3 {
            let r = rel + k as f32 * 0.07;
            let (x, y) = (0.5 - 0.085 * r.sin(), 0.5 - 0.085 * ASPECT * r.cos());
            rect(c, x, y, 0.007, 0.007 * ASPECT, [220, 20, 20, alpha]);
        }
    }
}

fn weather_state(c: &mut Cmd) -> (i32, i32, f32) {
    c.i("GetWeatherState", &[c.ptr(S_INT), c.ptr(S_INT2), c.ptr(S_V3)]);
    (c.read_i32(S_INT), c.read_i32(S_INT2), c.read_f32(S_V3))
}

fn name_tag(c: &mut Cmd, at: [f32; 3], name: &str) {
    let m = c.mem();
    crate::write_bytes(m, c.scratch + S_TEXT, b"STRING\0");
    let mut bytes: Vec<u8> = name.bytes().take(60).collect();
    bytes.push(0);
    crate::write_bytes(m, c.scratch + S_NAME, &bytes);
    let origin = c.vec_arg(1, [at[0], at[1], at[2] + 1.15]);
    c.i("SetDrawOrigin", &[origin, b(false)]);
    c.i("SetTextFont", &[i(4)]);
    c.i("SetTextScale", &[f(0.0), f(0.4)]);
    c.i("SetTextColour", &[i(255), i(255), i(255), i(235)]);
    c.i("SetTextCentre", &[b(true)]);
    c.i("SetTextOutline", &[]);
    c.i("BeginTextCommandDisplayText", &[c.ptr(S_TEXT)]);
    c.i("AddTextComponentSubStringPlayerName", &[c.ptr(S_NAME)]);
    c.i("EndTextCommandDisplayText", &[f(0.0), f(0.0), i(0)]);
    c.i("ClearDrawOrigin", &[]);
}

fn c_pos(c: &mut Cmd, ent: i32) -> [f32; 3] {
    c.v("GetEntityCoords", &[i(ent), b(false)])
}

fn local_state(c: &mut Cmd, game: &Game, ped: i32, pos: [f32; 3]) -> State {
    let mut s = State {
        ped_model: c.i("GetEntityModel", &[i(ped)]) as u32,
        pos,
        heading: c.f("GetEntityHeading", &[i(ped)]),
        vel: c.v("GetEntityVelocity", &[i(ped)]),
        weapon: c.i("GetSelectedPedWeapon", &[i(ped)]) as u32,
        health: c.i("GetEntityHealth", &[i(ped)]).clamp(0, 65535) as u16,
        armour: c.i("GetPedArmour", &[i(ped)]).clamp(0, 255) as u8,
        ..Default::default()
    };
    if c.i("IsPedDeadOrDying", &[i(ped), b(true)]) != 0 {
        s.flags |= proto::FLAG_DEAD;
    }
    if c.i("GetInteriorFromEntity", &[i(ped)]) != 0 {
        s.flags |= proto::FLAG_INSIDE;
    }
    if c.i("IsPedRagdoll", &[i(ped)]) != 0 {
        s.flags |= proto::FLAG_RAGDOLL;
    }

    let entering = c.i("GetVehiclePedIsTryingToEnter", &[i(ped)]);
    if entering != 0 && c.exists(entering) && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) == 0 {
        s.flags |= proto::FLAG_ENTERING;
        s.veh_model = c.i("GetEntityModel", &[i(entering)]) as u32;
        s.veh_pos = c.v("GetEntityCoords", &[i(entering), b(false)]);
        s.seat = c.i("GetSeatPedIsTryingToEnter", &[i(ped)]).clamp(-1, 6) as i8;
    }
    let player = c.i("PlayerId", &[]);

    const INPUT_AIM: i32 = 25;

    let armed = c.i("IsPedArmed", &[i(ped), i(4)]) != 0;
    let aiming = (armed && c.i("IsPlayerFreeAiming", &[i(player)]) != 0)
        || (armed && c.i("IsControlPressed", &[i(0), i(INPUT_AIM)]) != 0)
        || (armed && game.last_shot.elapsed() < Duration::from_millis(600));
    if aiming {
        s.flags |= proto::FLAG_AIMING;
        let cam = c.v("GetGameplayCamCoord", &[]);
        let rot = c.v("GetGameplayCamRot", &[i(2)]);
        let (pitch, yaw) = (rot[0].to_radians(), rot[2].to_radians());
        let dir = [-yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos(), pitch.sin()];
        s.aim = [cam[0] + dir[0] * 40.0, cam[1] + dir[1] * 40.0, cam[2] + dir[2] * 40.0];
    }
    let veh = if c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) != 0 { c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) } else { 0 };
    if veh != 0 {
        s.flags |= proto::FLAG_IN_VEHICLE;
        s.veh_model = c.i("GetEntityModel", &[i(veh)]) as u32;
        s.veh_pos = c.v("GetEntityCoords", &[i(veh), b(false)]);
        s.veh_rot = c.v_packed("GetEntityRotation", &[i(veh), i(2)]);
        s.veh_vel = c.v("GetEntityVelocity", &[i(veh)]);
        s.seat = (-1..6).find(|seat| c.i("GetPedInVehicleSeat", &[i(veh), i(*seat), b(false)]) == ped).unwrap_or(-1) as i8;

        s.veh_owner = game.avatars.iter().find(|(_, a)| a.veh == veh).map(|(id, _)| *id).unwrap_or(0);
        let (p1, p2) = (c.ptr(S_INT), c.ptr(S_INT2));
        c.i("GetVehicleColours", &[i(veh), p1, p2]);
        s.veh_colours = [c.read_i32(S_INT).clamp(0, 255) as u8, c.read_i32(S_INT2).clamp(0, 255) as u8];
        let h = |v: f32| v.clamp(-4000.0, 1000.0) as i16;
        s.veh_engine = h(c.f("GetVehicleEngineHealth", &[i(veh)]));
        s.veh_body = h(c.f("GetVehicleBodyHealth", &[i(veh)]));
        s.veh_tank = h(c.f("GetVehiclePetrolTankHealth", &[i(veh)]));
        s.veh_tyres = (0..6).filter(|w| c.i("IsVehicleTyreBurst", &[i(veh), i(*w), b(false)]) != 0).fold(0, |m, w| m | 1 << w);
        s.veh_windows = (0..8).filter(|w| c.i("IsVehicleWindowIntact", &[i(veh), i(*w)]) == 0).fold(0, |m, w| m | 1 << w);
        s.veh_dead = c.i("IsEntityDead", &[i(veh), b(false)]) != 0;
    }
    s
}

fn read_look(c: &mut Cmd, ped: i32) -> Look {
    let mut look = Look::default();
    for (k, comp) in look.comps.iter_mut().enumerate() {
        let k = k as i32;
        *comp = (
            c.i("GetPedDrawableVariation", &[i(ped), i(k)]).clamp(0, 65535) as u16,
            c.i("GetPedTextureVariation", &[i(ped), i(k)]).clamp(0, 255) as u8,
            c.i("GetPedPaletteVariation", &[i(ped), i(k)]).clamp(0, 255) as u8,
        );
    }
    for (k, prop) in look.props.iter_mut().enumerate() {
        let anchor = proto::PROP_SLOTS[k];
        *prop = (
            c.i("GetPedPropIndex", &[i(ped), i(anchor)]).clamp(-1, 32767) as i16,
            c.i("GetPropTextureIndex", &[i(ped), i(anchor)]).clamp(0, 255) as u8,
        );
    }
    look
}

fn apply_look(c: &mut Cmd, ped: i32, look: &Look) {
    for (k, (d, t, p)) in look.comps.iter().enumerate() {
        c.i("SetPedComponentVariation", &[i(ped), i(k as i32), i(*d as i32), i(*t as i32), i(*p as i32)]);
    }
    for (k, (idx, t)) in look.props.iter().enumerate() {
        let anchor = proto::PROP_SLOTS[k];
        if *idx < 0 {
            c.i("ClearPedProp", &[i(ped), i(anchor)]);
        } else {
            c.i("SetPedPropIndex", &[i(ped), i(anchor), i(*idx as i32), i(*t as i32), b(true)]);
        }
    }
}

fn local_shot(c: &mut Cmd, ped: i32, pos: [f32; 3]) -> Shot {
    let zero = c.vec_arg(2, [0.0; 3]);
    let hand = c.v("GetPedBoneCoords", &[i(ped), i(BONE_RIGHT_HAND), zero]);
    let from = if dist(hand, pos) < 3.0 { hand } else { pos };
    let hit = c.i("GetPedLastWeaponImpactCoord", &[i(ped), c.ptr(S_V3)]) != 0;
    let impact = [c.read_f32(S_V3), c.read_f32(S_V3 + 4), c.read_f32(S_V3 + 8)];
    let to = if hit && dist(impact, from) < 1500.0 {
        impact
    } else {
        let h = c.f("GetEntityHeading", &[i(ped)]).to_radians();
        [from[0] - 100.0 * h.sin(), from[1] + 100.0 * h.cos(), from[2]]
    };
    Shot { weapon: c.i("GetSelectedPedWeapon", &[i(ped)]) as u32, from, to }
}

fn bullet_on_driver(c: &mut Cmd, game: &mut Game, weapon_hash: u32, at: [f32; 3]) {
    if weapon_hash == weapon("weapon_stungun") {
        return;
    }

    const BONE_SPINE3: i32 = 24818;
    const BONE_HEAD: i32 = 31086;
    for a in game.avatars.values_mut() {
        if a.ped == 0 || a.dead || !c.exists(a.ped) || c.i("IsPedInAnyVehicle", &[i(a.ped), b(false)]) == 0 {
            continue;
        }
        let zero = c.vec_arg(2, [0.0; 3]);
        let lo = c.v("GetPedBoneCoords", &[i(a.ped), i(BONE_SPINE3), zero]);
        let zero = c.vec_arg(2, [0.0; 3]);
        let hi = c.v("GetPedBoneCoords", &[i(a.ped), i(BONE_HEAD), zero]);
        let veh = c.i("GetVehiclePedIsIn", &[i(a.ped), b(false)]);
        let speed = if veh != 0 { c.f("GetEntitySpeed", &[i(veh)]) } else { 0.0 };

        if seg_dist(at, lo, hi) < 0.3 + speed * 0.03 && at[2] > lo[2] - 0.1 {
            let dmg = c.f("GetWeaponDamage", &[i(weapon_hash as i32), i(0)]).clamp(5.0, 200.0) as u16;
            a.bullet = a.bullet.saturating_add(dmg);
        }
    }
}

fn seg_dist(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = if len2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1] + (p[2] - a[2]) * ab[2]) / len2).clamp(0.0, 1.0) } else { 0.0 };
    dist(p, [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t])
}

fn fire(c: &mut Cmd, a: &mut Avatar, shot: &Shot, me: Option<[f32; 3]>) {
    if !c.exists(a.ped) || a.dead {
        return;
    }

    const DAMAGE_MELEE: i32 = 2;
    if c.i("GetWeaponDamageType", &[i(shot.weapon as i32)]) == DAMAGE_MELEE {
        swing(c, a, shot.to);
        return;
    }
    if a.veh == 0 && a.walk.since.is_none() {
        let tgt = c.vec_arg(1, shot.to);
        c.i("TaskAimGunAtCoord", &[i(a.ped), tgt, i(800), b(false), b(false)]);
        a.aim_until = Some(Instant::now() + Duration::from_millis(800));
    }

    if c.i("HasWeaponAssetLoaded", &[i(shot.weapon as i32)]) == 0 {
        c.i("RequestWeaponAsset", &[i(shot.weapon as i32), i(31), i(0)]);
    }

    let mut target = shot.to;
    if let Some(me) = me {
        if dist(target, me) < 2.5 {
            target = [me[0] + (me[0] - shot.from[0]).signum() * 1.5, me[1] + (me[1] - shot.from[1]).signum() * 1.5, me[2] + 1.6];
        }
    }
    let t = c.vec_arg(3, target);
    c.i("SetPedShootsAtCoord", &[i(a.ped), t, b(true)]);
}

const PUNCHES: &[(&str, &str)] = &[
    ("melee@unarmed@streamed_core", "heavy_punch_a"),
    ("melee@unarmed@streamed_core", "short_0_punch"),
    ("melee@unarmed@streamed_variations", "plyr_takedown_front_slap"),
    ("melee@unarmed@streamed_core", "plyr_running_att"),
    ("melee@unarmed@streamed_core", "ground_att_on_spot"),
];

fn punch_anim(c: &mut Cmd) -> Option<(&'static str, &'static str)> {
    static PICK: Mutex<(usize, Option<usize>)> = Mutex::new((0, None));
    let mut pick = PICK.lock().unwrap();
    if let Some(k) = pick.1 {
        let (dict, clip) = PUNCHES[k];
        c.c_str(S_DICT, dict);
        if c.i("HasAnimDictLoaded", &[c.ptr(S_DICT)]) == 0 {
            c.i("RequestAnimDict", &[c.ptr(S_DICT)]);
            return None;
        }
        return Some((dict, clip));
    }
    while let Some(&(dict, clip)) = PUNCHES.get(pick.0) {
        c.c_str(S_DICT, dict);
        if c.i("DoesAnimDictExist", &[c.ptr(S_DICT)]) == 0 {
            pick.0 += 1;
            continue;
        }
        if c.i("HasAnimDictLoaded", &[c.ptr(S_DICT)]) == 0 {
            c.i("RequestAnimDict", &[c.ptr(S_DICT)]);
            return None;
        }
        c.c_str(S_CLIP, clip);
        if c.f("GetAnimDuration", &[c.ptr(S_DICT), c.ptr(S_CLIP)]) > 0.0 {
            eprintln!("[mp] punch animation: {dict} {clip}");
            pick.1 = Some(pick.0);
            return Some((dict, clip));
        }
        pick.0 += 1;
    }
    if pick.0 == PUNCHES.len() {
        eprintln!("[mp] no punch animation found");
        pick.0 += 1;
    }
    None
}

fn swing(c: &mut Cmd, a: &mut Avatar, to: [f32; 3]) {
    if a.veh != 0 {
        return;
    }
    let Some((dict, clip)) = punch_anim(c) else { return };
    let at = c.v("GetEntityCoords", &[i(a.ped), b(false)]);
    if dist(at, to) > 0.2 {
        c.i("SetEntityHeading", &[i(a.ped), f((-(to[0] - at[0])).atan2(to[1] - at[1]).to_degrees())]);
    }
    c.c_str(S_DICT, dict);
    c.c_str(S_CLIP, clip);
    const UPPER_BODY_SECONDARY: i32 = 16 | 32;
    c.i("TaskPlayAnim", &[i(a.ped), c.ptr(S_DICT), c.ptr(S_CLIP), f(8.0), f(-8.0), i(-1), i(UPPER_BODY_SECONDARY), f(0.0), b(false), i(0), b(false)]);
}

fn despawn(c: &mut Cmd, a: Avatar) {
    if a.blip != 0 {
        let r = c.ref_arg(a.blip);
        c.i("RemoveBlip", &[r]);
    }
    if c.exists(a.ped) {
        let r = c.ref_arg(a.ped);
        c.i("DeletePed", &[r]);
    }
    if c.exists(a.veh) {
        let r = c.ref_arg(a.veh);
        c.i("DeleteVehicle", &[r]);
    }
}

fn delete_vehicle(c: &mut Cmd, veh: i32) {
    if c.exists(veh) {
        if c.i("IsEntityAMissionEntity", &[i(veh)]) == 0 {
            c.i("SetEntityAsMissionEntity", &[i(veh), b(true), b(true)]);
        }
        let r = c.ref_arg(veh);
        c.i("DeleteVehicle", &[r]);
    }
}

static PARKED: Mutex<Vec<(i32, u32, Instant, u8)>> = Mutex::new(Vec::new());

const PARKED_KEEP: Duration = Duration::from_secs(300);

fn park(veh: i32, model: u32, owner: u8) {
    PARKED.lock().unwrap().push((veh, model, Instant::now(), owner));
}

fn unpark(veh: i32) {
    PARKED.lock().unwrap().retain(|p| p.0 != veh);
}

fn adopt_parked(c: &mut Cmd, model: u32, pos: [f32; 3]) -> Option<i32> {
    let list: Vec<(i32, u32, Instant, u8)> = PARKED.lock().unwrap().clone();
    let mut found = None;
    let mut keep = Vec::new();
    for (veh, m, t, owner) in list {
        if !c.exists(veh) || c.i("IsEntityDead", &[i(veh), b(false)]) != 0 {
            continue;
        }
        if t.elapsed() > PARKED_KEEP {
            release(c, veh);
            continue;
        }
        if found.is_none() && m == model && dist(c.v("GetEntityCoords", &[i(veh), b(false)]), pos) < 6.0 {
            found = Some(veh);
            continue;
        }
        keep.push((veh, m, t, owner));
    }
    *PARKED.lock().unwrap() = keep;
    found
}

fn tidy_parked(c: &mut Cmd, where_: &HashMap<u8, [f32; 3]>) {
    const KEEP_NEAR: f32 = 300.0;
    let list: Vec<(i32, u32, Instant, u8)> = PARKED.lock().unwrap().clone();
    let mut keep = Vec::new();
    for (veh, m, t, owner) in list {
        if !c.exists(veh) {
            continue;
        }
        if owner != 0 {
            let at = c.v("GetEntityCoords", &[i(veh), b(false)]);
            let far = where_.get(&owner).map(|p| dist(*p, at) > KEEP_NEAR).unwrap_or(true);
            if far && c.i("IsEntityOnScreen", &[i(veh)]) == 0 {
                delete_vehicle(c, veh);
                continue;
            }
        }
        keep.push((veh, m, t, owner));
    }
    *PARKED.lock().unwrap() = keep;
}

fn delete_ped(c: &mut Cmd, ped: i32) {
    if c.exists(ped) {
        let r = c.ref_arg(ped);
        c.i("DeletePed", &[r]);
    }
}

fn model_ready(c: &mut Cmd, model: u32) -> bool {
    if model == 0 {
        return false;
    }
    if c.i("HasModelLoaded", &[i(model as i32)]) != 0 {
        return true;
    }
    c.i("RequestModel", &[i(model as i32)]);
    false
}

fn players_group(c: &mut Cmd) -> i32 {
    static GROUP: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    let mut g = GROUP.load(Ordering::Relaxed);
    if g == 0 || c.i("DoesRelationshipGroupExist", &[i(g)]) == 0 {
        c.c_str(S_NAME, "MP_PLAYERS");
        c.i("AddRelationshipGroup", &[c.ptr(S_NAME), c.ptr(S_INT)]);
        g = c.read_i32(S_INT);
        GROUP.store(g, Ordering::Relaxed);
    }
    let me = c.i("PlayerPedId", &[]);
    if g != 0 && c.exists(me) {
        const DISLIKE: i32 = 4;
        let mine = c.i("GetPedRelationshipGroupHash", &[i(me)]);
        c.i("SetRelationshipBetweenGroups", &[i(DISLIKE), i(g), i(mine)]);
        c.i("SetRelationshipBetweenGroups", &[i(DISLIKE), i(mine), i(g)]);

        c.i("SetRelationshipGroupAffectsWantedLevel", &[i(g), b(false)]);
        c.i("SetCanAttackFriendly", &[i(me), b(true), b(false)]);
    }
    g
}

fn obedient_ped(c: &mut Cmd, model: u32, pos: [f32; 3], heading: f32) -> i32 {
    let p = c.vec_arg(1, pos);
    let ped = c.i("CreatePed", &[i(4), i(model as i32), p, f(heading), b(false), b(false)]);
    if ped != 0 {
        c.i("SetBlockingOfNonTemporaryEvents", &[i(ped), b(true)]);
        c.i("SetPedKeepTask", &[i(ped), b(true)]);
        c.i("SetPedCanBeDraggedOut", &[i(ped), b(false)]);
    }
    ped
}

fn update_avatar(c: &mut Cmd, owner: u8, a: &mut Avatar, s: &State, look: Option<Look>, cars: &HashMap<u8, i32>) {
    let died_here = a.ped != 0 && !a.dead && !s.dead() && c.exists(a.ped) && c.i("IsPedDeadOrDying", &[i(a.ped), b(true)]) != 0;
    if a.ped != 0 && (!c.exists(a.ped) || a.ped_model != s.ped_model || (a.dead && !s.dead()) || died_here) {
        let mut hit = a.hit;
        if died_here {
            let me = c.i("PlayerPedId", &[]);
            if me != 0 && (c.i("GetPedSourceOfDeath", &[i(a.ped)]) == me || c.i("HasEntityBeenDamagedByEntity", &[i(a.ped), i(me), b(true)]) != 0) {
                let unarmed = c.i("GetSelectedPedWeapon", &[i(me)]) as u32 == weapon("weapon_unarmed");
                hit = hit.saturating_add(if unarmed { 50 } else { 100 });
                eprintln!("[mp] finished avatar {} here: hit {hit}", a.ped);
            }
        }
        let respawned = a.dead && !s.dead();
        let old = std::mem::take(a);
        despawn(c, Avatar { veh: 0, ..old });
        if respawned && old.veh != 0 {
            if old.veh_damage.5 || c.i("IsEntityDead", &[i(old.veh), b(false)]) != 0 {
                release(c, old.veh);
            } else {
                delete_vehicle(c, old.veh);
            }
        } else {
            a.veh = old.veh;
            a.veh_model = old.veh_model;
        }
        a.hit = hit;
    }
    if a.ped == 0 {
        if s.dead() || !model_ready(c, s.ped_model) {
            return;
        }
        let ped = obedient_ped(c, s.ped_model, s.pos, s.heading);
        if ped == 0 {
            return;
        }
        a.ped = ped;
        a.ped_model = s.ped_model;
        c.i("SetPedDefaultComponentVariation", &[i(ped)]);

        c.i("SetPedMaxHealth", &[i(ped), i(AVATAR_HEALTH)]);

        c.i("SetEntityProofs", &[i(ped), b(false), b(true), b(true), b(false), b(false), b(true), b(false), b(false)]);
        c.i("SetEntityHealth", &[i(ped), i(AVATAR_HEALTH), i(0)]);
        c.i("SetPedCanRagdoll", &[i(ped), b(false)]);

        let group = players_group(c);
        if group != 0 {
            c.i("SetPedRelationshipGroupHash", &[i(ped), i(group)]);
        }
        c.i("SetPedCanBeTargetted", &[i(ped), b(true)]);
        a.blip = c.i("AddBlipForEntity", &[i(ped)]);
        if a.blip != 0 {
            c.i("ShowHeadingIndicatorOnBlip", &[i(a.blip), b(true)]);
        }
        eprintln!("[mp] spawned avatar ped {ped} (model {:#x})", s.ped_model);
    }
    let ped = a.ped;

    if look.is_some() && look != a.look {
        apply_look(c, ped, look.as_ref().unwrap());
        a.look = look;
    }
    if s.weapon != a.weapon {
        if s.weapon != weapon("weapon_unarmed") {
            c.i("GiveWeaponToPed", &[i(ped), i(s.weapon as i32), i(9999), b(false), b(true)]);
        }
        c.i("SetCurrentPedWeapon", &[i(ped), i(s.weapon as i32), b(true)]);
        a.weapon = s.weapon;
    }
    if !s.dead() && !a.dead {
        let h = c.i("GetEntityHealth", &[i(ped)]);
        if h < AVATAR_HEALTH && h > 0 {
            a.hit = a.hit.saturating_add((AVATAR_HEALTH - h).min(1000) as u16);
            c.i("SetEntityHealth", &[i(ped), i(AVATAR_HEALTH), i(0)]);
        }
    }
    if s.dead() {
        if !a.dead {
            a.dead = true;
            c.i("SetEntityInvincible", &[i(ped), b(false)]);
            c.i("SetPedCanRagdoll", &[i(ped), b(true)]);
            c.i("SetEntityHealth", &[i(ped), i(0), i(0)]);
        }
        return;
    }

    if s.in_vehicle() && s.veh_owner != 0 {
        delete_vehicle(c, std::mem::take(&mut a.veh));
        if let Some(&car) = cars.get(&s.veh_owner) {
            if c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) != car && c.i("IsVehicleSeatFree", &[i(car), i(s.seat as i32), b(false)]) != 0 {
                c.i("SetPedIntoVehicle", &[i(ped), i(car), i(s.seat as i32)]);
            }
        } else {
            let pos = c.vec_arg(1, s.pos);
            c.i("SetEntityCoordsNoOffset", &[i(ped), pos, b(false), b(false), b(false)]);
        }
        return;
    }

    if s.in_vehicle() {
        if a.veh != 0 && (!c.exists(a.veh) || a.veh_model != s.veh_model) {
            delete_vehicle(c, a.veh);
            a.veh = 0;
        }
        if a.veh == 0 {
            if !model_ready(c, s.veh_model) {
                let since = *a.veh_wait.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_secs(5) {
                    eprintln!("[mp] car model {:#x} still not loaded", s.veh_model);
                    a.veh_wait = Some(Instant::now());
                }
                return;
            }
            a.veh_wait = None;

            let sitting = c.i("GetVehiclePedIsIn", &[i(ped), b(false)]);
            let sitting = if sitting != 0 && c.exists(sitting) && c.i("GetEntityModel", &[i(sitting)]) as u32 == s.veh_model { Some(sitting) } else { None };
            if let Some(v) = sitting {
                unpark(v);
            }
            let veh = match sitting.or_else(|| adopt_parked(c, s.veh_model, s.veh_pos)) {
                Some(v) => v,
                None => {
                    let pos = c.vec_arg(1, s.veh_pos);
                    c.i("CreateVehicle", &[i(s.veh_model as i32), pos, f(s.veh_rot[2]), b(false), b(false), b(false)])
                }
            };
            if veh == 0 {
                return;
            }
            a.veh = veh;
            a.veh_model = s.veh_model;
            a.veh_colours = [255, 255];
            eprintln!("[mp] avatar car {veh} (model {:#x})", s.veh_model);
            a.veh_damage = (1000, 1000, 1000, 0, 0, false);
            a.veh_look = None;
            a.vctl = None;
            c.i("SetVehicleEngineOn", &[i(veh), b(true), b(true), b(false)]);
        }
        let veh = a.veh;
        if a.veh_colours != s.veh_colours {
            c.i("SetVehicleColour", &[i(veh), i(s.veh_colours[0] as i32), i(s.veh_colours[1] as i32)]);
            a.veh_colours = s.veh_colours;
        }

        let climbing = a.entering.map(|(v, t)| v == veh && t.elapsed() < Duration::from_secs(3)).unwrap_or(false);
        if c.i("GetVehiclePedIsIn", &[i(ped), b(false)]) != veh && !climbing {
            c.i("SetPedIntoVehicle", &[i(ped), i(veh), i(s.seat as i32)]);
        }
        if !climbing {
            a.entering = None;
        }

        if !a.veh_damage.5 && !s.veh_dead {
            if c.i("IsEntityDead", &[i(veh), b(false)]) != 0 {
                a.car_hit.4 = true;
                a.veh_damage.5 = true;
            }
        }
        if !a.veh_damage.5 {
            let engine = c.f("GetVehicleEngineHealth", &[i(veh)]) as i16;
            let body = c.f("GetVehicleBodyHealth", &[i(veh)]) as i16;
            let tank = c.f("GetVehiclePetrolTankHealth", &[i(veh)]) as i16;
            let tyres = (0..6).filter(|w| c.i("IsVehicleTyreBurst", &[i(veh), i(*w), b(false)]) != 0).fold(0u8, |m, w| m | 1 << w);
            let de = (a.veh_damage.0 - engine).max(0);
            let db = (a.veh_damage.1 - body).max(0);
            let dk = (a.veh_damage.2 - tank).max(0);
            let dt = tyres & !a.veh_damage.3;
            if de > 5 || db > 5 || dk > 5 || dt != 0 {
                a.car_hit = (a.car_hit.0.saturating_add(de), a.car_hit.1.saturating_add(db), a.car_hit.2 | dt, a.car_hit.3.saturating_add(dk), a.car_hit.4);
                c.i("SetVehiclePetrolTankHealth", &[i(veh), f(a.veh_damage.2 as f32)]);

                c.i("SetVehicleEngineHealth", &[i(veh), f(a.veh_damage.0 as f32)]);
                c.i("SetVehicleBodyHealth", &[i(veh), f(a.veh_damage.1 as f32)]);
            }
        }
        apply_car_damage(c, veh, &mut a.veh_damage, (s.veh_engine, s.veh_body, s.veh_tank, s.veh_tyres, s.veh_windows, s.veh_dead));
        steer_vehicle(c, veh, s.veh_pos, s.veh_rot, s.veh_vel, a.veh_spin);
        return;
    }

    let leaving = a.leaving_until.map(|t| Instant::now() < t).unwrap_or(false);
    if !leaving && (a.veh != 0 || c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) != 0) {
        let in_car = c.i("GetVehiclePedIsIn", &[i(ped), b(false)]);
        let slow = in_car != 0 && c.f("GetEntitySpeed", &[i(in_car)]) < 3.0;
        if slow && dist(c_pos(c, ped), s.pos) < 6.0 {
            c.i("TaskLeaveVehicle", &[i(ped), i(in_car), i(0)]);
            a.leaving_until = Some(Instant::now() + Duration::from_millis(2500));
        } else {
            let pos = c.vec_arg(1, s.pos);
            c.i("SetEntityCoordsNoOffset", &[i(ped), pos, b(false), b(false), b(false)]);
        }
        a.entering = None;
        let veh = std::mem::take(&mut a.veh);
        if a.veh_damage.5 {
            release(c, veh);
        } else if c.exists(veh) {
            park(veh, a.veh_model, owner);
        }
        a.walk.since = None;
    }
    if let Some(t) = a.leaving_until {
        if Instant::now() < t && c.i("IsPedInAnyVehicle", &[i(ped), b(false)]) != 0 && dist(c_pos(c, ped), s.pos) < 6.0 {
            return;
        }
        a.leaving_until = None;
    }

    if s.ragdoll() {
        let renew = a.ragdoll.map(|t| t.elapsed() > Duration::from_millis(1500)).unwrap_or(true);
        if renew {
            let first = a.ragdoll.is_none();
            c.i("SetPedCanRagdoll", &[i(ped), b(true)]);
            c.i("SetPedToRagdoll", &[i(ped), i(2500), i(2500), i(0), b(false), b(false), b(false)]);
            if first {
                let v = c.vec_arg(1, s.vel);
                c.i("SetEntityVelocity", &[i(ped), v]);
            }
            a.ragdoll = Some(Instant::now());
        }
        a.walk.since = None;
        if dist(c_pos(c, ped), s.pos) > 6.0 {
            let pos = c.vec_arg(1, s.pos);
            c.i("SetEntityCoordsNoOffset", &[i(ped), pos, b(false), b(false), b(false)]);
        }
        return;
    }
    if a.ragdoll.take().is_some() {
        c.i("SetPedCanRagdoll", &[i(ped), b(false)]);
    }

    if s.entering() {
        let busy = a.entering.map(|(v, t)| c.exists(v) && t.elapsed() < Duration::from_secs(6)).unwrap_or(false);
        if busy {
            return;
        }
        if let Some(v) = car_near(c, ped, s.veh_model, s.veh_pos) {
            if dist(c_pos(c, ped), c_pos(c, v)) < 10.0 {
                let speed = if s.vel[0].hypot(s.vel[1]) > 2.5 { 2.0 } else { 1.0 };
                c.i("TaskEnterVehicle", &[i(ped), i(v), i(6000), i(s.seat as i32), f(speed), i(1), Val::I64(0)]);
                a.entering = Some((v, Instant::now()));
                a.walk.since = None;
                return;
            }
        }
    } else if a.entering.take().is_some() {
        c.i("ClearPedTasks", &[i(ped)]);
        a.walk.since = None;
    }
    if s.aiming() {
        aim_walk(c, ped, &mut a.walk, s.pos, s.vel, s.aim);
        return;
    }
    if a.walk.aim.take().is_some() {
        c.i("ClearPedTasksImmediately", &[i(ped)]);
        a.walk.since = None;
    }
    let aiming = a.aim_until.map(|t| Instant::now() < t).unwrap_or(false);
    if !aiming {
        walk(c, ped, &mut a.walk, s.pos, s.vel, s.heading);
    }
}

fn car_near(c: &mut Cmd, ped: i32, model: u32, at: [f32; 3]) -> Option<i32> {
    let mut best: Option<(i32, f32)> = None;
    for v in c.nearby("GetNearbyVehicles", ped, S_VEHS, &[]) {
        if c.i("GetEntityModel", &[i(v)]) as u32 != model {
            continue;
        }
        let d = dist(c_pos(c, v), at);
        if d < 5.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((v, d));
        }
    }
    best.map(|(v, _)| v)
}

fn read_vctl(c: &mut Cmd, veh: i32) -> [u8; 4] {
    c.i("GetVehicleLightsState", &[i(veh), c.ptr(S_INT), c.ptr(S_INT2)]);
    let lights = if c.read_i32(S_INT2) != 0 { 2 } else if c.read_i32(S_INT) != 0 { 1 } else { 0 };
    let doors = (0..6).filter(|d| c.f("GetVehicleDoorAngleRatio", &[i(veh), i(*d)]) > 0.15).fold(0u8, |m, d| m | 1 << d);
    let player = c.i("PlayerId", &[]);

    let horn = c.i("IsHornActive", &[i(veh)]) != 0 || c.i("IsPlayerPressingHorn", &[i(player)]) != 0 || crate::pad::horn_held();
    [lights, horn as u8, (c.i("IsVehicleSirenOn", &[i(veh)]) != 0) as u8, doors]
}

fn apply_vctl(c: &mut Cmd, a: &mut Avatar, want: [u8; 4]) {
    let veh = a.veh;
    let have = a.vctl.unwrap_or([255, 0, 255, 0]);
    if have[0] != want[0] {
        c.i("SetVehicleLights", &[i(veh), i(if want[0] > 0 { 2 } else { 1 })]);
        c.i("SetVehicleFullBeam", &[i(veh), b(want[0] == 2)]);
    }
    if have[2] != want[2] {
        c.i("SetVehicleSiren", &[i(veh), b(want[2] != 0)]);
    }

    let busy = a.entering.is_some() || a.leaving_until.is_some();
    for d in 0..6 {
        if busy {
            break;
        }
        let (was, now) = (have[3] & 1 << d != 0, want[3] & 1 << d != 0);
        if was != now {
            if now {
                c.i("SetVehicleDoorOpen", &[i(veh), i(d), b(false), b(false)]);
            } else {
                c.i("SetVehicleDoorShut", &[i(veh), i(d), b(false)]);
            }
        }
    }

    if want[1] != 0 && a.horn_at.map(|t| t.elapsed() > Duration::from_millis(250)).unwrap_or(true) {
        c.i("SoundCarHorn", &[i(veh), i(350), i(proto::joaat("HELDDOWN") as i32), b(false)]);
        a.horn_at = Some(Instant::now());
    }
    a.vctl = Some(want);
}

fn read_vlook(c: &mut Cmd, veh: i32) -> VehLook {
    let (p1, p2) = (c.ptr(S_INT), c.ptr(S_INT2));
    c.i("GetVehicleExtraColours", &[i(veh), p1, p2]);
    let mut l = VehLook {
        model: c.i("GetEntityModel", &[i(veh)]) as u32,
        extra_colours: [c.read_i32(S_INT).clamp(0, 255) as u8, c.read_i32(S_INT2).clamp(0, 255) as u8],
        livery: c.i("GetVehicleLivery", &[i(veh)]).clamp(-1, 127) as i8,
        window_tint: c.i("GetVehicleWindowTint", &[i(veh)]).clamp(-1, 127) as i8,
        wheel_type: c.i("GetVehicleWheelType", &[i(veh)]).clamp(-1, 127) as i8,
        ..Default::default()
    };
    for k in 0..50 {
        l.mods[k] = c.i("GetVehicleMod", &[i(veh), i(k as i32)]).clamp(-1, 127) as i8;
    }
    for t in 17..=22 {
        if c.i("IsToggleModOn", &[i(veh), i(t)]) != 0 {
            l.toggles |= 1 << (t - 17);
        }
    }
    l.radio = c.i("GetPlayerRadioStationIndex", &[]).clamp(0, 255) as u8;
    if let Some(Val::I64(p)) = call(c.caller, "mp:GetVehicleNumberPlateText", &[i(veh)]) {
        if p != 0 {
            let text = read_cstr(c.mem(), p as u64);
            for (k, ch) in text.bytes().take(8).enumerate() {
                l.plate[k] = ch;
            }
        }
    }
    l
}

fn apply_vlook(c: &mut Cmd, veh: i32, l: &VehLook) {
    c.i("SetVehicleExtraColours", &[i(veh), i(l.extra_colours[0] as i32), i(l.extra_colours[1] as i32)]);
    if l.livery >= 0 {
        c.i("SetVehicleLivery", &[i(veh), i(l.livery as i32)]);
    }
    if l.window_tint >= 0 {
        c.i("SetVehicleWindowTint", &[i(veh), i(l.window_tint as i32)]);
    }
    if l.mods.iter().any(|m| *m >= 0) || l.toggles != 0 {
        c.i("SetVehicleModKit", &[i(veh), i(0)]);
        if l.wheel_type >= 0 {
            c.i("SetVehicleWheelType", &[i(veh), i(l.wheel_type as i32)]);
        }
        for (k, m) in l.mods.iter().enumerate() {
            if *m >= 0 {
                c.i("SetVehicleMod", &[i(veh), i(k as i32), i(*m as i32), b(false)]);
            }
        }
        for t in 17..=22 {
            c.i("ToggleVehicleMod", &[i(veh), i(t), b(l.toggles & (1 << (t - 17)) != 0)]);
        }
    }
    let n = l.plate.iter().position(|b| *b == 0).unwrap_or(8);
    if n > 0 {
        c.c_str(S_NAME, &String::from_utf8_lossy(&l.plate[..n]));
        c.i("SetVehicleNumberPlateText", &[i(veh), c.ptr(S_NAME)]);
    }
}

fn apply_car_damage(c: &mut Cmd, veh: i32, have: &mut (i16, i16, i16, u8, u8, bool), want: (i16, i16, i16, u8, u8, bool)) {
    if want == *have {
        return;
    }

    if want.0 >= 990 && want.1 >= 990 && (have.0 < 900 || have.1 < 900 || have.3 & !want.3 != 0) {
        c.i("SetVehicleFixed", &[i(veh)]);
        c.i("SetVehicleDeformationFixed", &[i(veh)]);
        *have = (1000, 1000, 1000, 0, 0, false);
    }
    let close = |a: i16, b: i16| (a - b).abs() < 5;
    if !close(want.0, have.0) {
        c.i("SetVehicleEngineHealth", &[i(veh), f(want.0 as f32)]);
    }
    if !close(want.1, have.1) {
        c.i("SetVehicleBodyHealth", &[i(veh), f(want.1 as f32)]);
    }
    if !close(want.2, have.2) {
        c.i("SetVehiclePetrolTankHealth", &[i(veh), f(want.2 as f32)]);
    }
    for w in 0..6 {
        if want.3 & !have.3 & (1 << w) != 0 {
            c.i("SetVehicleTyreBurst", &[i(veh), i(w), b(false), f(1000.0)]);
        }
    }
    for w in 0..8 {
        if want.4 & !have.4 & (1 << w) != 0 {
            c.i("SmashVehicleWindow", &[i(veh), i(w)]);
        }
    }
    if want.5 && !have.5 && c.i("IsEntityDead", &[i(veh), b(false)]) == 0 {
        c.i("ExplodeVehicle", &[i(veh), b(true), b(false)]);
    }
    *have = want;
}

fn release(c: &mut Cmd, ent: i32) {
    if c.exists(ent) {
        let r = c.ref_arg(ent);
        c.i("SetEntityAsNoLongerNeeded", &[r]);
    }
}

fn steer_vehicle(c: &mut Cmd, veh: i32, pos: [f32; 3], rot: [f32; 3], vel: [f32; 3], spin: f32) {
    let cur = c.v("GetEntityCoords", &[i(veh), b(false)]);
    let err = dist(cur, pos);
    if err > 6.0 {
        let p = c.vec_arg(1, pos);
        c.i("SetEntityCoordsNoOffset", &[i(veh), p, b(false), b(false), b(false)]);
        let r = c.vec_arg(2, rot);
        c.i("SetEntityRotation", &[i(veh), r, i(2), b(true)]);
        let v = c.vec_arg(3, vel);
        c.i("SetEntityVelocity", &[i(veh), v]);
        return;
    }
    let mut cur = cur;
    if err > 2.5 {
        let p = [cur[0] + (pos[0] - cur[0]) * 0.6, cur[1] + (pos[1] - cur[1]) * 0.6, cur[2] + (pos[2] - cur[2]) * 0.6];
        let pa = c.vec_arg(1, p);
        c.i("SetEntityCoordsNoOffset", &[i(veh), pa, b(false), b(false), b(false)]);
        cur = p;
    }

    const K: f32 = 5.0;
    let mut fix = [(pos[0] - cur[0]) * K, (pos[1] - cur[1]) * K, (pos[2] - cur[2]) * K];
    let len = (fix[0] * fix[0] + fix[1] * fix[1] + fix[2] * fix[2]).sqrt();
    if len > 12.0 {
        fix = fix.map(|v| v * 12.0 / len);
    }
    let v = c.vec_arg(3, [vel[0] + fix[0], vel[1] + fix[1], vel[2] + fix[2]]);
    c.i("SetEntityVelocity", &[i(veh), v]);

    let have = c.v_packed("GetEntityRotation", &[i(veh), i(2)]);
    let d = |a: f32, b: f32| (b - a + 540.0).rem_euclid(360.0) - 180.0;
    let (dp, dr, dy) = (d(have[0], rot[0]), d(have[1], rot[1]), d(have[2], rot[2]));
    if dy.abs() > 25.0 || dp.abs() > 12.0 || dr.abs() > 12.0 {
        let r = c.vec_arg(2, rot);
        c.i("SetEntityRotation", &[i(veh), r, i(2), b(true)]);
    } else {
        let yaw = (spin + dy * K).clamp(-360.0, 360.0).to_radians();
        let w = c.vec_arg(2, [dr.to_radians() * 2.0, dp.to_radians() * 2.0, yaw]);
        c.i("SetEntityAngularVelocity", &[i(veh), w]);
    }
}

fn aim_walk(c: &mut Cmd, ped: i32, w: &mut Walker, pos: [f32; 3], vel: [f32; 3], aim: [f32; 3]) {
    let cur = c.v("GetEntityCoords", &[i(ped), b(false)]);
    let err = dist(cur, pos);
    if err > 0.15 {
        let k = if err > 1.5 { 0.6 } else { 0.35 };
        let p = c.vec_arg(1, [cur[0] + (pos[0] - cur[0]) * k, cur[1] + (pos[1] - cur[1]) * k, pos[2]]);
        c.i("SetEntityCoordsNoOffset", &[i(ped), p, b(false), b(false), b(false)]);
    }
    let speed = (vel[0].powi(2) + vel[1].powi(2)).sqrt();
    let moved = w.aim.map(|(a, t)| dist(a, aim) > 1.5 || t.elapsed() > Duration::from_millis(700)).unwrap_or(true);
    if !moved && (speed < 0.3) == w.since.is_none() {
        return;
    }
    let at = c.vec_arg(2, aim);
    if speed < 0.3 {
        c.i("TaskAimGunAtCoord", &[i(ped), at, i(1500), b(false), b(true)]);
        w.since = None;
    } else {
        let to = c.vec_arg(1, [pos[0] + vel[0], pos[1] + vel[1], pos[2] + vel[2]]);
        let blend = if speed > 2.2 { 2.0 } else { 1.0 };
        c.i("TaskGoToCoordWhileAimingAtCoord", &[i(ped), to, at, f(blend), b(false), f(0.5), f(4.0), b(false), i(0), b(true), i(proto::joaat("FIRING_PATTERN_FULL_AUTO") as i32)]);
        w.since = Some(Instant::now());
    }
    w.aim = Some((aim, Instant::now()));
}

fn walk(c: &mut Cmd, ped: i32, w: &mut Walker, pos: [f32; 3], vel: [f32; 3], heading: f32) {
    let cur = c.v("GetEntityCoords", &[i(ped), b(false)]);
    let speed = (vel[0].powi(2) + vel[1].powi(2)).sqrt();
    let err = dist(cur, pos);
    if err > 5.0 || (speed < 0.3 && err > 2.0) {
        let p = c.vec_arg(1, pos);
        c.i("SetEntityCoordsNoOffset", &[i(ped), p, b(false), b(false), b(false)]);
        w.since = None;
    }
    if speed < 0.3 {
        if err > 0.4 && err <= 2.0 {
            if w.since.is_none() || dist(pos, w.target) > 0.3 {
                let tgt = c.vec_arg(1, pos);
                c.i("TaskGoStraightToCoord", &[i(ped), tgt, f(1.0), i(-1), f(heading), f(0.1)]);
                w.target = pos;
                w.since = Some(Instant::now());
            }
            return;
        }
        if w.since.take().is_some() {
            c.i("ClearPedTasks", &[i(ped)]);
        }
        c.i("SetEntityHeading", &[i(ped), f(heading)]);
        return;
    }

    if err > 0.3 {
        let k = if err > 2.0 { 0.4 } else if err > 0.8 { 0.12 } else { 0.05 };
        let p = c.vec_arg(2, [cur[0] + (pos[0] - cur[0]) * k, cur[1] + (pos[1] - cur[1]) * k, pos[2]]);
        c.i("SetEntityCoordsNoOffset", &[i(ped), p, b(false), b(false), b(false)]);
    }

    let target = [pos[0] + vel[0], pos[1] + vel[1], pos[2] + vel[2]];
    let behind = err > 0.8 && ((pos[0] - cur[0]) * vel[0] + (pos[1] - cur[1]) * vel[1]) > 0.0;
    let blend: f32 = if speed > 5.5 { 3.0 } else if speed > 2.2 { 2.0 } else { 1.0 };
    let blend = if behind { (blend + 1.0).min(3.0) } else { blend };
    let stale = w.since.map(|t| t.elapsed() > Duration::from_millis(200)).unwrap_or(true);
    if stale || dist(target, w.target) > 0.7 || w.blend != blend {
        let tgt = c.vec_arg(1, target);
        c.i("TaskGoStraightToCoord", &[i(ped), tgt, f(blend), i(-1), f(40000.0), f(0.0)]);
        w.target = target;
        w.blend = blend;
        w.since = Some(Instant::now());
    }
}

fn population(c: &mut Cmd, game: &mut Game, net: &Net, my_id: Option<u8>, ped: i32, my_pos: [f32; 3]) {
    let Some(me) = my_id else { return };

    let near: Vec<(u8, f32, bool)> = {
        let peers = net.peers.lock().unwrap();
        peers.iter().filter_map(|(id, p)| {
            let s = p.snaps.back()?.1;
            let at = if s.in_vehicle() { s.veh_pos } else { s.pos };
            Some((*id, dist(at, my_pos), s.inside()))
        }).collect()
    };
    let range = |id: u8| if game.pop_owner == Some(id) { SHARE_FAR } else { SHARE_NEAR };

    let me_inside = c.i("GetInteriorFromEntity", &[i(ped)]) != 0;
    let owner = near.iter().filter(|(id, d, inside)| !inside && (me_inside || *id < me) && *d < range(*id)).map(|(id, ..)| *id).min();
    if owner != game.pop_owner {
        match owner {
            Some(o) => {
                eprintln!("[mp] traffic and pedestrians here are player {o}'s");

                let p = c.vec_arg(1, my_pos);
                c.i("ClearAreaOfVehicles", &[p, f(SHARE_FAR), b(false), b(false), b(false), b(false), b(false), b(false), b(false)]);

                if !me_inside {
                    let p = c.vec_arg(1, my_pos);
                    c.i("ClearAreaOfPeds", &[p, f(SHARE_FAR), b(false)]);
                }
            }
            None => eprintln!("[mp] traffic and pedestrians here are ours"),
        }
        let old: Vec<Mirror> = game.mirrors.drain().map(|(_, m)| m).collect();
        for m in old {
            remove_mirror(c, m);
        }
        game.pop_owner = owner;
    }

    match owner {
        None => {
            let someone_near = near.iter().any(|(_, d, _)| *d < SHARE_FAR);
            if someone_near && game.last_pop.elapsed() >= POP_EVERY {
                game.last_pop = Instant::now();
                let others: Vec<[f32; 3]> = {
                    let peers = net.peers.lock().unwrap();
                    peers.values().filter_map(|p| p.snaps.back()).map(|(_, s)| if s.in_vehicle() { s.veh_pos } else { s.pos }).filter(|at| dist(*at, my_pos) < SHARE_FAR).collect()
                };
                let entries = read_population(c, game, ped, my_pos, &others, false);
                send_pop(net, &entries);
            }
        }
        Some(_) => {
            if game.last_pop.elapsed() >= POP_EVERY {
                game.last_pop = Instant::now();
                let others: Vec<[f32; 3]> = {
                    let peers = net.peers.lock().unwrap();
                    peers.values().filter_map(|p| p.snaps.back()).map(|(_, s)| if s.in_vehicle() { s.veh_pos } else { s.pos }).filter(|at| dist(*at, my_pos) < SHARE_FAR).collect()
                };
                let entries = read_population(c, game, ped, my_pos, &others, true);
                let mut payload = Vec::new();
                proto::encode_pop(&entries, &mut payload);
            unreplicated(net);
            }
        }
    }

    let sharing: Vec<u8> = near.iter().filter(|(_, d, _)| *d < SHARE_FAR).map(|(id, ..)| *id).collect();
    if owner.is_some() {
        c.i("SetVehicleDensityMultiplierThisFrame", &[f(0.0)]);
        c.i("SetRandomVehicleDensityMultiplierThisFrame", &[f(0.0)]);
        c.i("SetParkedVehicleDensityMultiplierThisFrame", &[f(0.0)]);
        c.i("SetPedDensityMultiplierThisFrame", &[f(0.0)]);
        c.i("SetScenarioPedDensityMultiplierThisFrame", &[f(0.0), f(0.0)]);
    }

    let lists: Vec<u8> = owner.into_iter().chain(sharing.iter().map(|o| o | EXTRAS)).collect();
    for o in lists {
        let entries = {
            let peers = net.peers.lock().unwrap();
            let list = |p: &Peer| if o & EXTRAS != 0 { p.extras.clone() } else { p.pop.clone() };
            peers.get(&(o & !EXTRAS)).and_then(list).filter(|(t, _)| t.elapsed() < Duration::from_secs(2)).map(|(t, e)| (t.elapsed(), e))
        };
        if let Some((age, mut entries)) = entries {
            let dt = age.as_secs_f32().min(0.3);
            for e in entries.iter_mut() {
                for k in 0..3 {
                    e.pos[k] += e.vel[k] * dt;
                }
            }
            mirror_population(c, game, net, o, &entries);
        }
    }

    let away: Vec<(u8, u32)> = game.mirrors.iter().filter(|(_, m)| m.seen.elapsed() > Duration::from_secs(6)).map(|(k, _)| *k).collect();
    for k in away {
        if let Some(m) = game.mirrors.remove(&k) {
            remove_mirror(c, m);
        }
    }
}

fn read_population(c: &mut Cmd, game: &Game, ped: i32, my_pos: [f32; 3], others: &[[f32; 3]], extras_only: bool) -> Vec<PopEntry> {
    let mine = c.i("GetVehiclePedIsIn", &[i(ped), b(false)]);
    let parked: Vec<i32> = if extras_only { PARKED.lock().unwrap().iter().map(|p| p.0).collect() } else { Vec::new() };
    let ours = |e: i32| e == ped || e == mine || game.avatars.values().any(|a| a.ped == e || a.veh == e) || game.mirrors.values().any(|m| m.ent == e || m.driver == e) || parked.contains(&e);

    let mut lookers = vec![ped];
    lookers.extend(game.avatars.values().filter(|a| a.ped != 0 && !a.dead && c.exists(a.ped) && dist(c.v("GetEntityCoords", &[i(a.ped), b(false)]), my_pos) < SHARE_FAR).map(|a| a.ped));
    let mut vehs: Vec<i32> = Vec::new();
    let mut peds: Vec<i32> = Vec::new();
    for l in &lookers {
        for v in c.nearby("GetNearbyVehicles", *l, S_VEHS, &[]) {
            if !vehs.contains(&v) {
                vehs.push(v);
            }
        }
        for p in c.nearby("GetNearbyPeds", *l, S_PEDS, &[i(-1)]) {
            if !peds.contains(&p) {
                peds.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for veh in vehs {
        if ours(veh) || c.i("IsEntityAMissionEntity", &[i(veh)]) != 0 {
            continue;
        }
        let pos = c.v("GetEntityCoords", &[i(veh), b(false)]);
        let driver = c.i("GetPedInVehicleSeat", &[i(veh), i(-1), b(false)]);
        out.push(PopEntry {
            key: veh as u32,
            is_vehicle: true,
            model: c.i("GetEntityModel", &[i(veh)]) as u32,
            driver_model: if driver != 0 { c.i("GetEntityModel", &[i(driver)]) as u32 } else { 0 },
            pos,
            rot: c.v_packed("GetEntityRotation", &[i(veh), i(2)]),
            vel: c.v("GetEntityVelocity", &[i(veh)]),
            dead: c.i("IsEntityDead", &[i(veh), b(false)]) != 0,
            engine: c.f("GetVehicleEngineHealth", &[i(veh)]).clamp(-4000.0, 1000.0) as i16,
            body: c.f("GetVehicleBodyHealth", &[i(veh)]).clamp(-4000.0, 1000.0) as i16,
        });
    }
    for p in peds {
        if ours(p) || c.i("IsEntityAMissionEntity", &[i(p)]) != 0 || c.i("IsPedInAnyVehicle", &[i(p), b(false)]) != 0 {
            continue;
        }
        out.push(PopEntry {
            key: p as u32,
            is_vehicle: false,
            model: c.i("GetEntityModel", &[i(p)]) as u32,
            driver_model: 0,
            pos: c.v("GetEntityCoords", &[i(p), b(false)]),
            rot: [0.0, 0.0, c.f("GetEntityHeading", &[i(p)])],
            vel: c.v("GetEntityVelocity", &[i(p)]),
            dead: c.i("IsPedDeadOrDying", &[i(p), b(true)]) != 0,
            engine: 0,
            body: 0,
        });
    }

    let near = |p: [f32; 3]| others.iter().chain(std::iter::once(&my_pos)).map(|o| dist(p, *o)).fold(f32::MAX, f32::min);
    out.sort_by(|a, b| near(a.pos).total_cmp(&near(b.pos)));
    let total = if extras_only { proto::POP_MAX } else { proto::POP_MAX * POP_PARTS };
    let cars_max = total * 11 / 20;
    let (mut cars, peds): (Vec<PopEntry>, Vec<PopEntry>) = out.into_iter().partition(|e| e.is_vehicle);
    cars.truncate(cars_max);
    let room = total - cars.len();
    cars.extend(peds.into_iter().take(room));
    cars
}

const POP_PARTS: usize = 3;

fn send_pop(net: &Net, entries: &[PopEntry]) {
    let chunks: Vec<&[PopEntry]> = if entries.is_empty() { vec![&[][..]] } else { entries.chunks(proto::POP_MAX).collect() };
    let parts = chunks.len() as u8;
    for (k, chunk) in chunks.into_iter().enumerate() {
        let mut payload = Vec::new();
        proto::encode_pop(chunk, &mut payload);
        payload.extend([k as u8, parts]);
            unreplicated(net);
    }
}

const EXTRAS: u8 = 0x80;

fn mirror_population(c: &mut Cmd, game: &mut Game, net: &Net, owner: u8, entries: &[PopEntry]) {
    let now = Instant::now();
    let mut spawned = 0;
    for e in entries {
        let key = (owner, e.key);
        if game.taken.contains(&key) {
            continue;
        }
        let alive = game.mirrors.get(&key).map(|m| c.exists(m.ent)).unwrap_or(false);
        if !alive {
            if let Some(m) = game.mirrors.remove(&key) {
                remove_mirror(c, m);
            }

            if e.dead || spawned >= 2 || !model_ready(c, e.model) || (e.is_vehicle && e.driver_model != 0 && !model_ready(c, e.driver_model)) {
                continue;
            }
            spawned += 1;
            let m = if e.is_vehicle {
                let veh = match adopt_parked(c, e.model, e.pos) {
                    Some(v) => v,
                    None => {
                        let p = c.vec_arg(1, e.pos);
                        c.i("CreateVehicle", &[i(e.model as i32), p, f(e.rot[2]), b(false), b(false), b(false)])
                    }
                };
                if veh == 0 {
                    continue;
                }
                c.i("SetVehicleEngineOn", &[i(veh), b(true), b(true), b(false)]);
                let driver = if e.driver_model != 0 { obedient_ped(c, e.driver_model, e.pos, e.rot[2]) } else { 0 };
                if driver != 0 {
                    c.i("SetPedIntoVehicle", &[i(driver), i(veh), i(-1)]);
                }
                Mirror { is_vehicle: true, dead: false, ent: veh, driver, walk: Walker::default(), seen: now, still: false }
            } else {
                let p = obedient_ped(c, e.model, e.pos, e.rot[2]);
                if p == 0 {
                    continue;
                }
                c.i("SetPedDefaultComponentVariation", &[i(p)]);
                Mirror { is_vehicle: false, dead: false, ent: p, driver: 0, walk: Walker::default(), seen: now, still: false }
            };
            game.mirrors.insert(key, m);
        }
        let Some(m) = game.mirrors.get_mut(&key) else { continue };
        m.seen = now;
        m.still = e.vel.iter().map(|v| v * v).sum::<f32>() < 1.0;
        if m.dead {
            continue;
        }
        let dead_here = if e.is_vehicle { c.i("IsEntityDead", &[i(m.ent), b(false)]) != 0 } else { c.i("IsPedDeadOrDying", &[i(m.ent), b(true)]) != 0 };
        if e.dead || dead_here {
            m.dead = true;
            if e.dead && !dead_here {
                if e.is_vehicle {
                    c.i("ExplodeVehicle", &[i(m.ent), b(true), b(false)]);
                } else {
                    c.i("SetEntityHealth", &[i(m.ent), i(0), i(0)]);
                }
            } else if !e.dead {
                let mut payload = vec![owner & !EXTRAS];
                payload.extend(e.key.to_le_bytes());
                send_reliable(net, proto::KILL, &payload, Some(owner & !EXTRAS));
            }
            continue;
        }
        if e.is_vehicle {
            let have = (c.f("GetVehicleEngineHealth", &[i(m.ent)]) as i16, c.f("GetVehicleBodyHealth", &[i(m.ent)]) as i16);
            if (have.0 - e.engine).abs() > 25 {
                c.i("SetVehicleEngineHealth", &[i(m.ent), f(e.engine as f32)]);
            }
            if (have.1 - e.body).abs() > 25 {
                c.i("SetVehicleBodyHealth", &[i(m.ent), f(e.body as f32)]);
            }
            steer_vehicle(c, m.ent, e.pos, e.rot, e.vel, 0.0);
        } else {
            let mut w = m.walk;
            walk(c, m.ent, &mut w, e.pos, e.vel, e.rot[2]);
            m.walk = w;
        }
    }

    let stale: Vec<(u8, u32)> = game.mirrors.iter().filter(|(k, m)| k.0 == owner && now.duration_since(m.seen) > Duration::from_secs(if m.still { 6 } else { 1 })).map(|(k, _)| *k).collect();
    for k in stale {
        if let Some(m) = game.mirrors.remove(&k) {
            remove_mirror(c, m);
        }
    }
}

static AVATAR_ENTS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

fn remove_mirror(c: &mut Cmd, m: Mirror) {
    let keep = AVATAR_ENTS.lock().unwrap().clone();
    if keep.contains(&m.ent) {
        if !keep.contains(&m.driver) {
            delete_ped(c, m.driver);
        }
        return;
    }
    if m.dead && c.exists(m.ent) && c.i("IsEntityOnScreen", &[i(m.ent)]) != 0 {
        release(c, m.driver);
        release(c, m.ent);
        return;
    }
    delete_ped(c, m.driver);
    if m.is_vehicle {
        delete_vehicle(c, m.ent);
    } else {
        delete_ped(c, m.ent);
    }
}
