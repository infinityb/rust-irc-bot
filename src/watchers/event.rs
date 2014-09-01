use message::IrcMessage;

use watchers::base::{
    MessageWatcher,
    EventWatcher,
    Bundler,
    BundlerTrigger
};
use watchers::join::JoinResult;


pub enum IrcEvent {
    // TODO: Why is IrcMessage boxed?
    IrcEventMessage(IrcMessage),
    IrcEventJoinBundle(JoinResult),
    IrcEventWatcherResponse(Box<MessageWatcher+Send>)
}
