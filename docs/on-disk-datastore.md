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



## Diffing - Deltas of Commits

Multiple commits are kept on disk in a compressed form. All commits except for the first (base) commit 
and the current commit are kept as compressed deltas against the previous commit (which will also be kept as delta
except for the first one). This leads to a chain of decompressions required when accessing a historic commit. 
An exception is the current commit, which is stored in uncompressed form, to allow for quick commits. 
In other words, commits are always performed in time O(1) yet restore operations may be slower 
because of the need for multiple chained decompressions.
Initial measurements indicate that to restore a commit when there are 200 commits present in the chain
takes around 1.67 seconds per module. Since session commits are collections of module commits,
restore performance is dependent on the number of modules.
Note that nothing is stored upon commit for modules that have not been active during the committed session, yet
restore will recover memory images of all modules that have been historically active.

Compressed deltas are stored in a similar way to full commits, yet they have an ordinal number appended
to them so that an example folder looks as follows:

```
/tmp/001:
589824 Feb 13 15:23 0100000000000000000000000000000000000000000000000000000000000000
327680 Feb 13 15:22 0100000000000000000000000000000000000000000000000000000000000000_E651BB6F2F3C2B4E13731EABFF750DFD9E3C8D57DCE2E59C66C7DB8A9B9D3F7E
 10132 Feb 13 15:22 0100000000000000000000000000000000000000000000000000000000000000_8A88E5D1819EE34EA0FD4BF6ED4E2752C290E852E82683E7EDF03710C56F9ACA_0
   156 Feb 13 15:23 0100000000000000000000000000000000000000000000000000000000000000_8A88E5D1819EE34EA0FD4BF6ED4E2752C290E852E82683E7EDF03710C56F9ACA_1
589824 Feb 13 15:23 0100000000000000000000000000000000000000000000000000000000000000_F1B5A3B5C4BF745EDA2FD552BE1F288C2FAE5B0A6E4C8528C334F150FE96E39E
327680 Feb 13 15:23 0200000000000000000000000000000000000000000000000000000000000000
262144 Feb 13 15:22 0200000000000000000000000000000000000000000000000000000000000000_E0B6CB2E08FDF46272F64BA637486C4FE2AF212CFFEC6E302C4FFAFF6D23FAB6
   180 Feb 13 15:22 0200000000000000000000000000000000000000000000000000000000000000_7185C75BD9CBD45301E9B1D6D9B5AD0F63E08B638FA387822CB99146B4E74AD7_0
   180 Feb 13 15:23 0200000000000000000000000000000000000000000000000000000000000000_7185C75BD9CBD45301E9B1D6D9B5AD0F63E08B638FA387822CB99146B4E74AD7_1
  2136 Feb 13 15:22 0200000000000000000000000000000000000000000000000000000000000000_F5D8C72CDA46DE6304B317AB34D21D27D0AF579680C1776B9639D5D5E522F0F4_0
   172 Feb 13 15:23 0200000000000000000000000000000000000000000000000000000000000000_F5D8C72CDA46DE6304B317AB34D21D27D0AF579680C1776B9639D5D5E522F0F4_1
327680 Feb 13 15:23 0200000000000000000000000000000000000000000000000000000000000000_10BB44EC62B192F96A1ED1A0165F82159E7FF3497B35245C55277A2C0CC0C451
```

Postfixes for compressed delta files are needed for the case when module commit ids happen to be not unique. 
This should rarely happen, the above example is not typical, and in normal case only prefixes `_0` should occur.
Nevertheless, prefixes are necessary if deltas happen to have the same id in order to prevent them from
overwriting each other. 
