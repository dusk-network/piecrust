


## On-disk store

### Initial State

Assume we create a vm with a base directory path "/tmp/001".
We deploy two modules with ids ModId1 and ModId2.
We perform some transactions which change the state of memory,
the state will be reflected in memory backing files also named ModId1 and ModId2 respectively.
In real life ModId1 and ModId2 look like 64-characters long hexadecimal numbers.
At this state the on-disk store looks as follows:


| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId2        | mmap backing file          |


### After Session Commit 1

When we commit a session, modules' states are stored in respective files.
In assition, files ModId1_last and ModId2_last are crated.
From now on, on module instantiation, "last" files' content will be loaded
to memory instead of normal memory initialization.

| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId1_Commit1|                            |
|                                   | ModId1_last   | points to commit1          |
|                                   | ModId2        | mmap backing file          |
|                                   | ModId2_Commit1|                            |
|                                   | ModId2_last   | points to commit1          |

Here is how the contents of the directory will look like in real life:
```
/tmp/001:
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_E3FB8F23757660D140CD6E9945B29DF2C37FE2C40D39D236E1A5339151C5671C
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_last
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_591FF54F19C2783CB4EE07E0DA90A4D1572ED5ABBEC4C766A27A90E75C325BBA
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_last
```


### After Session Commit 2

When we commit another session, addional files are corresponding modules are created,
and the "last" files are updated to point to this last commit

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

VM Persist step leaves the on-disk storage unchanged with an exception that it
adds a file named "commits" containing the Commits Store (described below) 

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
|                                   | commits       | contains commits' store    |


### After VM Restore Commit 1

Restore to Commit1 step changes contents of the "last" files so that they
point again to Commit1.

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
|                                   | commits       | contains commits' store    |



## Commits Store (On-disk as file 'commits' and in memory)

### Initial State

Until the first session commit is done, the commits' store is empty.

### After Session Commit 1

After first commit, a map of maps is created, with session commits as primary keys, 
and module ids as secondary keys. Commits' store allows for retrieving the information
about, for a given session commit, which module commits correspond to particular modules.

| Session Commits                   | Module Ids    | Module Commits             |
|-----------------------------------|---------------|----------------------------|
| Commit1                           | ModId1        | ModCommit1                 |
|                                   | ModId2        | ModCommit1                 |


### After Session Commit 2

After second commit, a map of maps is further enriched with second commit.
A particular module id has different module commits asigned depending on the session
commit id requested.

| Session Commits                   | Module Ids    | Module Commits             |
|-----------------------------------|---------------|----------------------------|
| Commit1                           | ModId1        | ModCommit1                 |
|                                   | ModId2        | ModCommit1                 |
| Commit2                           | ModId1        | ModCommit2                 |
|                                   | ModId2        | ModCommit2                 |


### After VM Persist

VM persist does not change the contents of Commits' store, but it saves it to disk
into a file named 'commits'.

### After VM Restore Commit 1

VM restore does not change the contents of Commits' store, but it loads contents
of a file named 'commits' into memory.

