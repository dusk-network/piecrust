


## On-disk store

### Initial State

| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId2        | mmap backing file          |


### After Session Commit 1

| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId1_Commit1|                            |
|                                   | ModId1_last   | points to commit1          |
|                                   | ModId2        | mmap backing file          |
|                                   | ModId2_Commit1|                            |
|                                   | ModId2_last   | points to commit1          |


### After Session Commit 2

| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|-------------------         |
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId1_Commit1|                            |
|                                   | ModId1_Commit2|                            |
|                                   | ModId1_last   | points to commit2          |
|                                   | ModId2        | mmap backing file          |
|                                   | ModId2_Commit1|                            |
|                                   | ModId2_Commit2|                            |
|                                   | ModId2_last   | points to commit2          |


### After VM Persist

| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId1_Commit1|                            |
|                                   | ModId1_Commit2|                            |
|                                   | ModId1_last   | points to commit2          |
|                                   | ModId2        | mmap backing file          |
|                                   | ModId2_Commit1|                            |
|                                   | ModId2_Commit2|                            |
|                                   | ModId2_last   | points to commit2          |
|                                   | commits       | contains commits' metadata |


### After VM Restore Commit 1

| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId1_Commit1|                            |
|                                   | ModId1_Commit2|                            |
|                                   | ModId1_last   | points to commit1          |
|                                   | ModId2        | mmap backing file          |
|                                   | ModId2_Commit1|                            |
|                                   | ModId2_Commit2|                            |
|                                   | ModId2_last   | points to commit1          |
|                                   | commits       | contains commits' metadata |



## Commits Store (On-disk as file 'commits' and in memory)

### Initial State

Empty

### After Session Commit 1

| Session Commits                   | Module Ids    | Module Commits             |
|-----------------------------------|---------------|----------------------------|
| Commit1                           | ModId1        | ModCommit1                 |
|                                   | ModId2        | ModCommit1                 |


### After Session Commit 2

| Session Commits                   | Module Ids    | Module Commits             |
|-----------------------------------|---------------|----------------------------|
| Commit1                           | ModId1        | ModCommit1                 |
|                                   | ModId2        | ModCommit1                 |
| Commit2                           | ModId1        | ModCommit2                 |
|                                   | ModId2        | ModCommit2                 |


### After VM Persist

No change

### After VM Restore Commit 1

No change
