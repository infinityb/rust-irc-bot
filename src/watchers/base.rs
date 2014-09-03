use message::IrcMessage;
use watchers::event::IrcEvent;


pub trait MessageWatcher {
    fn on_message(&mut self, message: &IrcMessage);

    /// If true, the `MessageWatcher` should be removed from the watcher set
    fn finished(&self) -> bool;
}


pub trait EventWatcher{
    fn on_event(&mut self, message: &IrcEvent);

    /// If true, the `EventWatcher` should be removed from the watcher set
    fn is_finished(&self) -> bool;

    fn get_name(&self) -> &'static str;
}


pub trait Bundler {
    fn on_message(&mut self, message: &IrcMessage) -> Vec<IrcEvent>;

    /// If true, the `Bundler` should be removed from the bundler set
    fn is_finished(&mut self) -> bool;
}


pub trait BundlerTrigger {
	fn on_message(&mut self, message: &IrcMessage) -> Vec<Box<Bundler+Send>>;
}