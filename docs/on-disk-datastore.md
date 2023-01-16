# On-Disk Store

VM uses on disk store to manage commits and state persistence.
A session can perform one commit before it ceases to exist.
The commit is stored on disk and can be restored any time by another session.
Session commits are always stored on disk yet their ids will not survive system restart.
VM persist and restore mechanism allow for commits to be preserved when system is restarted.
What follows is a more thorough explanation of how the on-disk store functions
as we progress with session commits, restore and VM persist and restore functions.

### Initial State

Assume that we create a VM with a base directory path "/tmp/001".
We deploy two modules with identifiers ModId1 and ModId2.
We perform some transactions which change the state of memory,
the state will be reflected in memory backing files which are also named ModId1 and ModId2 respectively.
In real life ModId1 and ModId2 will look more like 64-characters long hexadecimal numbers.
At this state the on-disk store looks as follows:


| Base directory                    | Files         | Comment                    |
|-----------------------------------|---------------|----------------------------|
| /tmp/001                          | ModId1        | mmap backing file          |
|                                   | ModId2        | mmap backing file          |

Here is how the contents of the directory will look like in real life:
```
/tmp/001:
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80
```

### After Session Commit 1

When we commit a session, modules' states are stored in respective files.
In addition, files ModId1_last and ModId2_last are crated, as well as
ModId1_last_id and ModId2_last_id.
From now on, on module instantiation, corresponding files with postfix "_last" will be loaded
to memory instead of normal memory initialization.
Files with postfix "last_id" contain current last module commit id,
which takes part in a merkle root of the VM's state calculation.

| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_last   | points to commit1                         |
|                                   | ModId1_last_id| contains id of commit1 for module ModId1  |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_last   | points to commit1                         |
|                                   | ModId2_last   | contains id of commit1 for module ModId2  |

Note that by "points to commitN", it is currently meant that the file has the same content as commitN file.
In future implementations this semantics will be preserved yet actual technical mechanism will differ,
for example, symbolic link or a postfix name will be used instead.

Here is how the contents of the directory will look like in real life:
```
/tmp/001:
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_E3FB8F23757660D140CD6E9945B29DF2C37FE2C40D39D236E1A5339151C5671C
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_last
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_last_id
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_591FF54F19C2783CB4EE07E0DA90A4D1572ED5ABBEC4C766A27A90E75C325BBA
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_last
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_last_id
```


### After Session Commit 2

When we commit yet another session, additional commit files for the corresponding modules are created,
and the "last" files are updated to point to these last commit files.

| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_Commit2|                                           |
|                                   | ModId1_last   | points to commit2                         |
|                                   | ModId1_last_id| contains id of commit2 for module ModId1  |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_Commit2|                                           |
|                                   | ModId2_last   | points to commit2                         |
|                                   | ModId2_last_id| contains id of commit2 for module ModId2  |

Here is how the contents of the directory will look like in real life:
```
/tmp/001:
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_17698D259DC35B01ECB4D676DE11B69FAD37B57EEA98045A6022E97EA2CEFB43
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_E3FB8F23757660D140CD6E9945B29DF2C37FE2C40D39D236E1A5339151C5671C
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_last
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_last_id
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_591FF54F19C2783CB4EE07E0DA90A4D1572ED5ABBEC4C766A27A90E75C325BBA
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_D1BD8323DDC7B9B124EE2E8A47E65863DBCB98CAAB580A328D044C3801F974B2
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_last
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_last_id
```

### After VM Persist

VM Persist step leaves the on-disk storage unchanged with an exception that it
adds a file named "commits" containing the Commits Store (described below). 

| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_Commit2|                                           |
|                                   | ModId1_last   | points to commit2                         |
|                                   | ModId1_last   | contains id of commit2 for module ModId1  |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_Commit2|                                           |
|                                   | ModId2_last   | points to commit2                         |
|                                   | ModId2_last   | contains id of commit2 for module ModId2  |
|                                   | commits       | contains commits' store                   |


### After VM Restore Commit 1

Restore to Commit1 step changes contents of the "last" files so that they
point again to Commit1. Note that now:
1) ModId1_last points to ModId1_Commit1
2) ModId2_last points to ModId2_Commit1

 
From now on, upon module instantiation, Commit1 content will be loaded rather than Commit2.


| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_Commit2|                                           |
|                                   | ModId1_last   | points to commit1                         |
|                                   | ModId1_last   | contains id of commit1 for module ModId1  |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_Commit2|                                           |
|                                   | ModId2_last   | points to commit1                         |
|                                   | ModId2_last   | contains id of commit1 for module ModId2  |
|                                   | commits       | contains commits' store                   |



## Commits Store (On-disk as file 'commits' and in memory)

In addition to module's state, we also need to store information about commits for
particular modules for particular session commits. This database, a map of maps, 
will be referred to as Commits Store. Below we explain how Commits Store is utilized.

### Initial State

Until the first session commit is done, Commits Store is empty.

### After Session Commit 1

After first commit, a map of maps is created, with session commits as primary keys, 
and module ids as secondary keys. Commits Store allows for retrieving the following information:
for a given session commit - which module commits correspond to particular modules.

| Session Commits                   | Module Ids    | Module Commits             |
|-----------------------------------|---------------|----------------------------|
| Commit1                           | ModId1        | ModCommit1                 |
|                                   | ModId2        | ModCommit1                 |


### After Session Commit 2

After second commit, a map of maps is further enriched with second commit.
A particular module id has different module commits assigned depending on the session
commit id requested.

| Session Commits                   | Module Ids    | Module Commits             |
|-----------------------------------|---------------|----------------------------|
| Commit1                           | ModId1        | ModCommit1                 |
|                                   | ModId2        | ModCommit1                 |
| Commit2                           | ModId1        | ModCommit2                 |
|                                   | ModId2        | ModCommit2                 |


### After VM Persist

VM persist does not change the contents of Commits Store, it only saves it to disk
into a file named 'commits'.

### After VM Restore Commit 1

VM restore does not change the contents of Commits Store. It loads the contents
of a file named 'commits' into memory instance of Commits Store.
