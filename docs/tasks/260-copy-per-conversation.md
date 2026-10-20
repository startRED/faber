# Each conversation in its own copy (parity P3)

Row P3 of docs/product/parity.md: a project can run every new conversation in its own separate copy on its own branch,
not only some, and the conversation's head shows which copy and branch it works in. Extends the separate copy (the
`copies/<id>/` stored value); the copy is removed with its conversation as today.

Done when: engine tests of the copy made for each new conversation and of its removal; a screen check of the head; the
disk cost of a copy is stated; the product map and docs/design are updated; a what's-new entry in five languages.
