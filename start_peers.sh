#!/bin/bash

# A name for our tmux session
SESSION_NAME="qemu_peers"

# The base command
BASE_CMD="sudo cargo make --no-workspace"

# Start a new, detached tmux session.
# The first command runs in the initial window.
tmux new-session -d -s $SESSION_NAME "sh -lc '$BASE_CMD qemu_peer_1 & tmux wait-for -S p1_started; wait'"

# Split the window vertically and run the second command in the new pane.
tmux split-window -h -t $SESSION_NAME "sh -lc 'tmux wait-for p1_started; sleep 3; $BASE_CMD qemu_peer_2'"


# Optional: Use a layout that distributes panes evenly.
tmux select-layout -t $SESSION_NAME even-horizontal

# Attach to the session to see your running processes.
tmux attach-session -t $SESSION_NAME

echo "tmux session '$SESSION_NAME' closed."