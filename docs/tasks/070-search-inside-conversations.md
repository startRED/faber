# Search finds words inside conversations

Parity row F1 ([parity.md](../product/parity.md#finding-and-keeping)), planned for phase 2 in docs/product/README.md.
The sidebar's search also finds the words of messages, in both areas, and opens the conversation at the message. The
engine keeps a small index of `conversations/*.json`, built when it starts in the background at idle priority and
updated when a message is saved; no new process, and its memory is measured against R125.

Done:
- engine tests over fixtures: a word in the middle of an answer is found, a removed conversation is not;
- tests/ui/ checks a hit opens the conversation at the message;
- the product map (Sidebar row, stored state if the index is a file), parity.md without F1, the memory cost in the
  commit body, and a what's-new line in every language.
