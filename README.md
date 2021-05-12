# Git + EcoStructure Machine Expert - Basic

This repo contains tools for managing `Machine Expert - Basic` projects in
a Git repo.

Currently, there is a textconv filter that provides
* Context for diff chunk headers
* Normalization of UUIDs, to make the diffs more readable
* Hides the ladder diagram sections, only showing the PLC code.
* Pretty-printing of PLC code, instead of the original XML-section-per-line format.

## Installation
Clone the repo locally and run `cargo install --path .`

Add the following section to .git/config
```yaml
[diff "plc"]
  textconv = plc-textconv
  xfuncname = "ctx=.*\""
```
and put `*.smbp eol=crlf diff=plc` in .git/attributes.
