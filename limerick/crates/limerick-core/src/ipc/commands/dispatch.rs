//! Master command dispatch — routes [`Command`] variants to handler functions.

use crate::input::{AtmosphericTopic, Command};
use crate::npc::manager::NpcManager;
use crate::world::WorldState;

use super::flags::handle_flag_command;
use super::help::render_help_text;
use super::info::handle_info_command;
use super::listen::handle_atmospheric_command;
use super::map::{handle_map_command, handle_unexplored_command};
use super::provider::{
    handle_category_provider_command, handle_cloud_provider_command, handle_preset_command,
    handle_provider_command,
};
use super::session::handle_session_command;
use super::theme::handle_theme_command;
use super::time::handle_time_control_command;
use super::toggles::handle_sidebar_improv_command;
use super::weather::handle_weather_command;
use super::{CommandEffect, CommandResult};
use crate::ipc::config::GameConfig;

/// Processes a system command, mutating world/NPC/config state and returning
/// the response text plus any side effects.
///
/// The caller must acquire whatever locks are necessary before calling this
/// function and handle the returned [`CommandEffect`]s afterwards.
pub fn handle_command(
    cmd: Command,
    world: &mut WorldState,
    npc_manager: &mut NpcManager,
    config: &mut GameConfig,
) -> CommandResult {
    match cmd {
        Command::Pause
        | Command::Resume
        | Command::PauseSilent
        | Command::ResumeSilent
        | Command::Status
        | Command::ShowSpeed
        | Command::SetSpeed(_)
        | Command::InvalidSpeed(_)
        | Command::Wait(_)
        | Command::Tick => handle_time_control_command(cmd, world, npc_manager, config),

        Command::About | Command::NpcsHere | Command::Time => {
            handle_info_command(cmd, world, npc_manager)
        }

        Command::ToggleSidebar | Command::ToggleImprov => {
            handle_sidebar_improv_command(cmd, config)
        }

        Command::ShowProvider
        | Command::SetProvider(_)
        | Command::ShowModel
        | Command::SetModel(_)
        | Command::ShowKey
        | Command::SetKey(_)
        | Command::ShowBaseUrl
        | Command::SetBaseUrl(_) => handle_provider_command(cmd, config),

        Command::ShowCloud
        | Command::SetCloudProvider(_)
        | Command::ShowCloudModel
        | Command::SetCloudModel(_)
        | Command::ShowCloudKey
        | Command::SetCloudKey(_) => handle_cloud_provider_command(cmd, config),

        Command::ShowCategoryProvider(_)
        | Command::SetCategoryProvider(_, _)
        | Command::ShowCategoryModel(_)
        | Command::SetCategoryModel(_, _)
        | Command::ShowCategoryKey(_)
        | Command::SetCategoryKey(_, _)
        | Command::ShowCategoryBaseUrl(_)
        | Command::SetCategoryBaseUrl(_, _) => handle_category_provider_command(cmd, config),

        Command::ShowPreset | Command::ApplyPreset(_) => handle_preset_command(cmd, config),

        Command::Flags
        | Command::Flag(_)
        | Command::InvalidFlagName(_)
        | Command::InvalidBranchName(_)
        | Command::InvalidSystemCommand(_) => handle_flag_command(cmd, config),

        Command::Quit => CommandResult::effect_only(CommandEffect::Quit),
        Command::Help => CommandResult::text_tabular(render_help_text()),
        Command::Save => CommandResult::effect_only(CommandEffect::SaveGame),
        Command::Fork(name) => CommandResult::effect_only(CommandEffect::ForkBranch(name)),
        Command::Load(name) => CommandResult::effect_only(CommandEffect::LoadBranch(name)),
        Command::Branches => CommandResult::effect_only(CommandEffect::ListBranches),
        Command::Log => CommandResult::effect_only(CommandEffect::ShowLog),
        Command::Map(arg) => handle_map_command(config, arg),
        Command::Unexplored(arg) => handle_unexplored_command(config, arg),
        Command::Weather(arg) => handle_weather_command(world, arg),
        Command::Listen => handle_atmospheric_command(world, config, AtmosphericTopic::Listen),
        Command::Omen => handle_atmospheric_command(world, config, AtmosphericTopic::Omen),
        Command::Folklore => handle_atmospheric_command(world, config, AtmosphericTopic::Folklore),
        Command::Session => handle_session_command(world, config),
        Command::Designer => CommandResult::effect_only(CommandEffect::OpenDesigner),
        Command::Debug(sub) => CommandResult::effect_only(CommandEffect::Debug(sub)),
        Command::Spinner(secs) => CommandResult::effect_only(CommandEffect::ShowSpinner(secs)),
        Command::NewGame => CommandResult::effect_only(CommandEffect::NewGame),
        Command::Theme(arg) => handle_theme_command(arg),
        Command::ResetByok => {
            CommandResult::with_effect("Re-opening provider picker...", CommandEffect::ResetByok)
        }
        Command::InferenceLog(sub) => CommandResult::effect_only(CommandEffect::InferenceLog(sub)),
    }
}
