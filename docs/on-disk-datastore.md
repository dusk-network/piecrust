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

| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |


Here is how the contents of the directory will look like in real life:
```
/tmp/001:
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_E3FB8F23757660D140CD6E9945B29DF2C37FE2C40D39D236E1A5339151C5671C
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_591FF54F19C2783CB4EE07E0DA90A4D1572ED5ABBEC4C766A27A90E75C325BBA
```


### After Session Commit 2

When we commit yet another session, additional commit files for the corresponding modules are created.

| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_Commit2|                                           |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_Commit2|                                           |

Here is how the contents of the directory will look like in real life:
```
/tmp/001:
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_17698D259DC35B01ECB4D676DE11B69FAD37B57EEA98045A6022E97EA2CEFB43
912D69F5B63ECFDE1F70F25C45AD810D191D3381DC3012FB807FF926098A209E_E3FB8F23757660D140CD6E9945B29DF2C37FE2C40D39D236E1A5339151C5671C
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_591FF54F19C2783CB4EE07E0DA90A4D1572ED5ABBEC4C766A27A90E75C325BBA
A2937AF0F0137F912F7569E28B2773563160DD99460A7E4DAA78E08B2DABFA80_D1BD8323DDC7B9B124EE2E8A47E65863DBCB98CAAB580A328D044C3801F974B2
```

### After VM Persist

VM Persist step leaves the on-disk storage unchanged with an exception that it
adds a file named "commits" containing the Commits Store (described below). 

| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_Commit2|                                           |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_Commit2|                                           |
|                                   | commits       | contains commits' store                   |


### After VM Restore Commit 1

Restore to Commit1 does not cause any visible changes in the directory.
The actual change is done only inside the commits file.

 
From now on, upon module instantiation, Commit1 content will be loaded rather than Commit2,
yet directory contents look the same.


| Base directory                    | Files         | Comment                                   |
|-----------------------------------|---------------|-------------------------------------------|
| /tmp/001                          | ModId1        | mmap backing file                         |
|                                   | ModId1_Commit1|                                           |
|                                   | ModId1_Commit2|                                           |
|                                   | ModId2        | mmap backing file                         |
|                                   | ModId2_Commit1|                                           |
|                                   | ModId2_Commit2|                                           |
|                                   | commits       | contains commits' store                   |



## Commits Store (On-disk as file 'commits' and in memory)

In addition to module's state, we also need to store information about commits for
particular modules for particular session commits. This database will be referred to as Commits Store. 


Per session Commits Store keeps the following information:

- current commit id - so that system is able to restore commit id after a cold reboot
- current module commit ids per module - so the system is abe to restore module commit ids after a cold reboot

In addition, there is also a per VM (global) commits store which keeps the following information:
- current session commit id
- historic session commits
- database of per-module ordered commit collections - global per-module collections of module states kept in as compressed deltas of subsequent commits. Only first and last commit states are kept uncompressed, all the intermediate states are compressed.
