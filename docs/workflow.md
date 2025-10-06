## part 1: pretool

given you start with this state

```
❯ jj log
@  utlsuunt code@schpet.com 2025-10-04 16:24:18 ec5b7625
│  (empty) uwc
○  tnnpnnts code@schpet.com 2025-10-04 16:24:08 git_head() 7e8b221a
│  (empty) base
◆  zzzzzzzz root() 00000000
```

when a claude session pretool hook fires, run

```sh
jj new -m "jjagent: precommit {short_session_id}"
```

> note: short_session_id is the first 8 chars of a claude session id

leaving you in this state:

```sh
❯ jj log
@  xlyysnuk code@schpet.com 2025-10-04 16:28:24 1e0a077c
│  (empty) jjagent: precommit abcd1234
○  utlsuunt code@schpet.com 2025-10-04 16:24:18 git_head() ec5b7625
│  (empty) uwc
○  tnnpnnts code@schpet.com 2025-10-04 16:24:08 7e8b221a
│  (empty) base
◆  zzzzzzzz root() 00000000
```

## part 2: posttool

### check for existing session change, or make one

check if there is a commit in your descendants with the Claude-session-id: trailer, note the closest one if there is one.

if there is NOT a commit in your descendants with the Claude-session-id: trailer, make one BEFORE @- :

```sh
jj new --insert-before @- --no-edit -m "jjagent: session {short_session_id}\n\nClaude-session-id: {full_session_id}"
```

leaving us at this state:

```
❯ jj log
@  xlyysnuk code@schpet.com 2025-10-04 16:38:16 d0a11c50
│  (empty) jjagent: precommit abcd1234
○  utlsuunt code@schpet.com 2025-10-04 16:38:16 git_head() dcdb5471
│  (empty) uwc
○  wmopswmq code@schpet.com 2025-10-04 16:38:11 7525f6ec
│  (empty) jjagent: session abcd1234
○  tnnpnnts code@schpet.com 2025-10-04 16:24:08 7e8b221a
│  (empty) base
◆  zzzzzzzz root() 00000000
```

TODO: reference jj trailers docs

### attempt to squash

count the conflicts on or after the `jjagent: session` change and store that for later

1. edit the uwc commit
2. squash the `jjagent: precommit` change into the `jjagent: session abcd1234` change, keeping the destination message

```sh
jj edit utlsuunt
jj squash --from xlyysnuk --into wmopswmq --use-destination-message
```

count the conflicts on or after the `jjagent: session` change.

are there any new conlicts introduced from teh squash?

#### if there are NOT conflicts

we are done!

```
@  utlsuunt code@schpet.com 2025-10-05 07:19:47 0f874099
│  (empty) uwc
○  wmopswmq code@schpet.com 2025-10-05 07:19:47 git_head() ac7e6930
│  (empty) jjagent: session abcd1234
○  tnnpnnts code@schpet.com 2025-10-04 16:24:08 7e8b221a
│  (empty) base
◆  zzzzzzzz root() 00000000
```

### if there ARE conflicts

`jj undo` both the squash, and the edit, so `@` is back on the jjagent: precommit  log

```
@  xlyysnuk code@schpet.com 2025-10-04 16:38:16 d0a11c50
│  (empty) jjagent: precommit abcd1234
○  utlsuunt code@schpet.com 2025-10-04 16:38:16 git_head() dcdb5471
│  (empty) uwc
○  wmopswmq code@schpet.com 2025-10-04 16:38:11 7525f6ec
│  (empty) jjagent: session abcd1234
○  tnnpnnts code@schpet.com 2025-10-04 16:24:08 7e8b221a
│  (empty) base
◆  zzzzzzzz root() 00000000
```

then, change the description from `jjagent: precommit {short_id}`  to `jjagent: session {short_id} pt. 2\n\nClaude-session-id: {full_session_id}`

```
@  xlyysnuk code@schpet.com 2025-10-05 07:26:11 caa2739c
│  (empty) jjagent: session abcd1234 pt. 2
○  utlsuunt code@schpet.com 2025-10-04 16:38:16 git_head() dcdb5471
│  (empty) uwc
○  wmopswmq code@schpet.com 2025-10-04 16:38:11 7525f6ec
│  (empty) jjagent: session abcd1234
○  tnnpnnts code@schpet.com 2025-10-04 16:24:08 7e8b221a
│  (empty) base
◆  zzzzzzzz root() 00000000
```

and then finally, run jj new to start a new working copy for the user's changes
