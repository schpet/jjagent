**Note**: This project uses [bd (beads)](https://github.com/steveyegge/beads) for issue tracking. Use `bd` commands instead of markdown TODOs. See AGENTS.md for workflow details.

claude code hooks to integrate with jujutsu vcs (jj). it isolates claude code sessions to a session specific jj change id.

## notes

- the goal is to have a linear history, with claude sessions tracked in their own changes, as well as the users changes isolated from claudes - no branching should occur, the users working copy should always be on top of all the changes
- conflicts should be avoided: if a jj command is run, and a conflict is created, use jj undo to go back to the previous state
- if we need to read structured information from a commit or change description/message (e.g. a claude code session id) always parse it from the trailer and not the title (which also may include it)
- in general errors should either be handled or panic, but not be silently dropped
- when snapshot testing, you can look at diffs for a given change via description with `jj log -r 'description(glob:"your description*")' -p`
- if asked to review jjagent logs, they are in ~/.cache/jjagent/jjagent.jsonl

## workflow

- Be sure to run tests and address clippy changes when you’re done making a series of code changes: `cargo clippy --all-targets --all-features -- -D warnings`

## Running Tests

Run all tests:
```bash

cargo test
# less output
cargo test --quiet

# Run a specific test:
cargo test test_first_tool_use
```

## manually verifying

if needed to test behavior, create test jj repos in /tmp and run claude for a real test

```bash
# Start the conversation and capture the session ID
initial_output=$(claude -p "This is the very first prompt" --output-format json)
SESSION_ID=$(echo "$initial_output" | jq -r '.session_id')

echo "Started session: $SESSION_ID"

# Now, for all future calls in your script, use that ID
claude -r "$SESSION_ID" -p "This is the second prompt"
claude -r "$SESSION_ID" -p "This is the third prompt"
```

## jj

### jj glossary

Anonymous branch - A chain of commits without bookmarks that jj retains until explicitly abandoned, with their visible heads tracked by the view.
Backend - An implementation of the storage layer, typically the Git backend for commits, with other backends for non-commit data like the operation log.
Bookmark - A named pointer to a commit; there is no "current" bookmark, they don’t advance on commit creation, but they follow rewrites of their target.
Branch - In jj, usually an anonymous branch (a path in the commit graph); local Git branches map to jj bookmarks, especially in colocated repos.
Change - A commit considered across its rewrites; not a separate object, identified by a change ID stored on commits.
Change ID - Unique identifier for a change (typically 16 bytes), shown as 12 k–z “digits” in jj log.
Commit - A snapshot of the repo (tree) plus metadata and parent pointers forming a DAG, often treated as its diff against parent(s), and synonymous with “revision.”
Commit ID - Unique identifier for a commit (20 bytes with the Git backend), displayed as 12 hex digits and equal to the Git SHA when using Git.
Colocated repos - A jj repo whose backing .git directory sits alongside .jj, enabling seamless mixed use of jj and git.
Conflict - A state requiring manual resolution, commonly in files but also in bookmarks or independently rewritten changes (divergent changes).
Divergent change - A single change that has multiple visible commits (e.g., rewritten independently).
Head - A commit with no descendants in a chosen set; the view tracks visible anonymous heads and this is unrelated to Git’s HEAD.
Hidden commits, abandoned commits - Commits no longer visible (e.g., abandoned or superseded by rewrites) but still addressable by commit ID.
Operation - A recorded snapshot of the view (visible commits, bookmarks, working copies) with metadata and parent pointers.
Operation log - The DAG of operations, linear for sequential actions and branching/merging when operations occur concurrently.
Repository - All data under .jj, comprising the complete set of operations and commits.
Remote - A reference to another copy of the repository (local or networked), compatible with Git remotes and hosting providers.
Revision - Synonym for a commit.
Revset - An expression language (and its result) for selecting sets of revisions.
Rewrite - Creating a new commit to replace another—changing contents and/or metadata—yielding a new commit ID while usually preserving the change ID (e.g., amend, rebase).
Root commit - A virtual initial commit with all-zero commit ID and all-z change ID, addressable as root() and distinct from Git’s first commits.
Tree - An object representing a directory snapshot, recursively including files and subtrees.
Tracked bookmarks and tracking bookmarks - Marking a remote bookmark as tracked creates a local tracking bookmark that follows it.
Visible commits - Commits reachable from anonymous heads recorded in the view (including their ancestors); abandoned or superseded commits become hidden and are no longer reachable by change ID.
View - A snapshot of bookmarks, anonymous heads, and working-copy commits that defines which commits are visible.
Workspace - A working copy plus its associated repository; multiple workspaces can share one repo, with secondary .jj directories pointing to the initial one.
Working copy - Your editable file tree that jj snapshots at command boundaries, creating or updating the working-copy commit.
Working-copy commit - The per-workspace commit mirroring the working copy’s current state, tracked in the operation log.

### jj man pages

run bash `man jj` or `man jj-<subcommand>` to get up to date details on jj

SUBCOMMANDS
       jj-abandon(1)
              Abandon a revision

       jj-absorb(1)
              Move changes from a revision into the stack of mutable revisions

       jj-bookmark(1)
              Manage bookmarks [default alias: b]

       jj-commit(1)
              Update the description and create a new change on top [default
              alias: ci]

       jj-config(1)
              Manage config options

       jj-describe(1)
              Update the change description or other metadata [default alias:
              desc]

       jj-diff(1)
              Compare file contents between two revisions

       jj-diffedit(1)
              Touch up the content changes in a revision with a diff editor

       jj-duplicate(1)
              Create new changes with the same content as existing ones

       jj-edit(1)
              Sets the specified revision as the working-copy revision

       jj-evolog(1)
              Show how a change has evolved over time

       jj-file(1)
              File operations

       jj-fix(1)
              Update files with formatting fixes or other changes

       jj-git(1)
              Commands for working with Git remotes and the underlying Git
              repo

       jj-help(1)
              Print this message or the help of the given subcommand(s)

       jj-interdiff(1)
              Compare the changes of two commits

       jj-log(1)
              Show revision history

       jj-metaedit(1)
              Modify the metadata of a revision without changing its content

       jj-new(1)
              Create a new, empty change and (by default) edit it in the
              working copy

       jj-next(1)
              Move the working-copy commit to the child revision

       jj-operation(1)
              Commands for working with the operation log

       jj-parallelize(1)
              Parallelize revisions by making them siblings

       jj-prev(1)
              Change the working copy revision relative to the parent revision

       jj-rebase(1)
              Move revisions to different parent(s)

       jj-redo(1)
              Redo the most recently undone operation

       jj-resolve(1)
              Resolve conflicted files with an external merge tool

       jj-restore(1)
              Restore paths from another revision

       jj-revert(1)
              Apply the reverse of the given revision(s)

       jj-root(1)
              Show the current workspace root directory (shortcut for `jj
              workspace root`)

       jj-show(1)
              Show commit description and changes in a revision

       jj-sign(1)
              Cryptographically sign a revision

       jj-simplify-parents(1)
              Simplify parent edges for the specified revision(s)

       jj-sparse(1)
              Manage which paths from the working-copy commit are present in
              the working copy

       jj-split(1)
              Split a revision in two

       jj-squash(1)
              Move changes from a revision into another revision

       jj-status(1)
              Show high-level repo status [default alias: st]

       jj-tag(1)
              Manage tags

       jj-undo(1)
              Undo the last operation

       jj-unsign(1)
              Drop a cryptographic signature

       jj-util(1)
              Infrequently used commands such as for generating shell
              completions

       jj-version(1)
              Display version information

       jj-workspace(1)
              Commands for working with workspaces

### jj commit trailers

Commit trailers

You can configure automatic addition of one or more trailers to commit descriptions using the commit_trailers template.

Each line of the template is an individual trailer, usually in Key: Value format.

Trailers defined in this template are deduplicated with the existing description: if the entire line of a trailer is already present, it will not be added again. To deduplicate based only on the trailer key, use the trailers.contains_key(key) method within the template.

```
[templates]
commit_trailers = '''
format_signed_off_by_trailer(self)
++ if(!trailers.contains_key("Change-Id"), format_gerrit_change_id_trailer(self))'''
Some ready-to-use trailer templates are available for frequently used trailers:
```

Existing trailers are also accessible via commit.trailers().
