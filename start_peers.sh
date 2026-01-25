#!/bin/bash

# A name for our tmux session
SESSION_NAME="qemu_peers"

# The base command
BASE_CMD="sudo cargo make --no-workspace --profile production"

CMD_1="($BASE_CMD qemu_peer_1 2>&1 | tee peer_1.log) & tmux wait-for -S p1_started; wait"

# Start a new, detached tmux session.
# The first command runs in the initial window.
tmux new-session -d -s $SESSION_NAME "sh -lc '$CMD_1'"

tmux set-option -t $SESSION_NAME history-limit 50000
tmux set-option -t $SESSION_NAME mouse on

CMD_2="tmux wait-for p1_started; sleep 3; $BASE_CMD qemu_peer_2 2>&1 | tee peer_2.log"

# Split the window vertically and run the second command in the new pane.
tmux split-window -h -t $SESSION_NAME "sh -lc '$CMD_2'"

# Layout and attach
tmux select-layout -t $SESSION_NAME even-horizontal
tmux attach-session -t $SESSION_NAME

echo "tmux session '$SESSION_NAME' closed."