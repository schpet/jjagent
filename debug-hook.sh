#!/bin/bash
# Debug wrapper for jjcc hooks

echo "========== JJCC HOOK DEBUG ==========" >> /tmp/jjcc-debug.log
echo "Date: $(date)" >> /tmp/jjcc-debug.log
echo "Hook type: $1" >> /tmp/jjcc-debug.log
echo "PWD: $PWD" >> /tmp/jjcc-debug.log
echo "Checking jj root..." >> /tmp/jjcc-debug.log
jj root >> /tmp/jjcc-debug.log 2>&1
echo "STDIN content:" >> /tmp/jjcc-debug.log
tee -a /tmp/jjcc-debug.log | jjcc hooks "$@" 2>> /tmp/jjcc-debug.log
echo "Exit code: $?" >> /tmp/jjcc-debug.log
echo "=====================================" >> /tmp/jjcc-debug.log