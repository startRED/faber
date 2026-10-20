# The commands running in the background (parity P1)

Row P1 of docs/product/parity.md: a view in the running column lists the commands the agent started that are still
running (both CLIs report them), with their last output lines and a Stop for each. Extends the running column, like the
helpers; no terminal is opened, and the output kept is capped.

Done when: engine tests from fake CLI recordings (a command starts, prints, is stopped, ends by itself); a screen check
opens the view and stops one; the memory cost is measured; the product map and docs/design are updated; a what's-new
entry in five languages.
