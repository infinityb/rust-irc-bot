use message::IrcMessage;

use watchers::base::{
    MessageWatcher,
    EventWatcher,
    Bundler,
    BundlerTrigger
};
use watchers::join::JoinResult;
use watchers::who::WhoResult;


pub enum IrcEvent {
    // TODO: Why is IrcMessage boxed?
    IrcEventMessage(IrcMessage),
    IrcEventJoinBundle(JoinResult),
    IrcEventWhoBundle(WhoResult),
    IrcEventWatcherResponse(Box<MessageWatcher+Send>)
}
