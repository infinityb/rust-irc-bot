
all:
	rustc -o rust-irc connector.rs 


test:
	rustc --test -o rust-irc-test connector.rs 
	./rust-irc-test