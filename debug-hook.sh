#!/bin/bash
# Debug wrapper for jjagent hooks

echo "========== JJAGENT HOOK DEBUG ==========\" >> /tmp/jjagent-debug.log
echo "Date: $(date)" >> /tmp/jjagent-debug.log
echo "Hook type: $1" >> /tmp/jjagent-debug.log
echo "PWD: $PWD" >> /tmp/jjagent-debug.log
echo "Checking jj root..." >> /tmp/jjagent-debug.log
jj root >> /tmp/jjagent-debug.log 2>&1
echo "STDIN content:" >> /tmp/jjagent-debug.log
tee -a /tmp/jjagent-debug.log | jjagent hooks "$@" 2>> /tmp/jjagent-debug.log
echo "Exit code: $?" >> /tmp/jjagent-debug.log
echo "=====================================" >> /tmp/jjagent-debug.log