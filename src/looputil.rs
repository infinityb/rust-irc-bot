use std::collections::HashSet;

use irc::legacy::State;
use irc::legacy::IrcMsg;

use command_mapper::PluginContainer;


pub fn register_plugins(plugins: &mut PluginContainer, enabled_plugins: &HashSet<String>) {
    use plugins::{
        DeerPlugin,
        GreedPlugin,
        SeenPlugin,
        RadioPlugin,
        PingPlugin,
        WserverPlugin,
        WhoAmIPlugin,
        LoggerPlugin,
        FetwgrkifgPlugin,
        AsciiArtPlugin,
        AnimeCalendarPlugin,
        UnicodeNamePlugin,
        EightBallPlugin,
        PickPlugin,
        IrcColorsPlugin,
    };

    if enabled_plugins.contains(PingPlugin::get_plugin_name()) {
        plugins.register(PingPlugin::new());
    }
    if enabled_plugins.contains(GreedPlugin::get_plugin_name()) {
        plugins.register(GreedPlugin::new());
    }
    if enabled_plugins.contains(SeenPlugin::get_plugin_name()) {
        plugins.register(SeenPlugin::new());
    }
    if enabled_plugins.contains(DeerPlugin::get_plugin_name()) {
        plugins.register(DeerPlugin::new());
    }
    if enabled_plugins.contains(RadioPlugin::get_plugin_name()) {
        plugins.register(RadioPlugin::new());
    }
    if enabled_plugins.contains(WserverPlugin::get_plugin_name()) {
        plugins.register(WserverPlugin::new());
    }
    if enabled_plugins.contains(WhoAmIPlugin::get_plugin_name()) {
        plugins.register(WhoAmIPlugin::new());
    }
    if enabled_plugins.contains(LoggerPlugin::get_plugin_name()) {
        plugins.register(LoggerPlugin::new());
    }
    if enabled_plugins.contains(FetwgrkifgPlugin::get_plugin_name()) {
        plugins.register(FetwgrkifgPlugin::new());
    }
    if enabled_plugins.contains(AsciiArtPlugin::get_plugin_name()) {
        plugins.register(AsciiArtPlugin::new());
    }
    if enabled_plugins.contains(UnicodeNamePlugin::get_plugin_name()) {
        plugins.register(UnicodeNamePlugin::new());
    }
    if enabled_plugins.contains(AnimeCalendarPlugin::get_plugin_name()) {
        plugins.register(AnimeCalendarPlugin::new());
    }
    if enabled_plugins.contains(EightBallPlugin::get_plugin_name()) {
        plugins.register(EightBallPlugin::new());
    }
    if enabled_plugins.contains(PickPlugin::get_plugin_name()) {
        plugins.register(PickPlugin::new());
    }
    if enabled_plugins.contains(IrcColorsPlugin::get_plugin_name()) {
        plugins.register(IrcColorsPlugin::new());
    }
}


pub struct StatePlugin {
    initial_nick: String,
    in_isupport: bool,
    isupport_finished: bool,
    emitted_state: bool,
}

impl StatePlugin {
    pub fn new() -> StatePlugin {
        StatePlugin {
            initial_nick: String::new(),
            in_isupport: false,
            isupport_finished: false,
            emitted_state: false,
        }
    }

    pub fn on_irc_msg(&mut self, msg: &IrcMsg) -> Option<State> {
        if self.emitted_state {
            return None;
        }

        let args = msg.get_args();

        if msg.get_command() == "001" {
            self.initial_nick = ::std::str::from_utf8(args[0]).unwrap().to_string();
        }
        if msg.get_command() == "005" {
            self.in_isupport = true;
        }

        // FIXME: ISUPPORT is required atm. We need an alternate way to determine
        // whether the IRC connection has ``completed''.
        if self.in_isupport && msg.get_command() != "005" {
            self.in_isupport = false;
            self.isupport_finished = true;
        }

        if self.initial_nick.len() > 0 && self.isupport_finished {
            let mut state = State::new();
            state.set_self_nick(&self.initial_nick);
            self.emitted_state = true;
            return Some(state);
        }

        None
    }
}
