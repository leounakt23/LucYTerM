# Feature Proposals

Feature proposals are lightweight RFCs for changes that cross trust boundaries,
add protocols or dependencies, alter persisted/public interfaces, require
ongoing services, or are likely to take more than one release cycle. The
canonical process, including the two-week discussion period and one-week final
comment period, lives in `rfcs/README.md`; existing proposals here remain valid
and follow that same process.

Start with a Feature Request Discussion. After a maintainer confirms that an
RFC is useful, copy the template to `NNNN-short-name.md`, fill every section,
and open a pull request. The number is assigned during review; use `0000` in
drafts. An RFC records a decision but does not reserve implementation capacity
or promise a release.

Proposal states are `Draft`, `Review`, `Accepted`, `Rejected`, `Withdrawn`, and
`Superseded`. Accepted proposals name an implementation owner, tracking issue,
milestone candidate, and review date. Material design changes amend the RFC or
use a superseding proposal. Security-sensitive details belong in a private
GitHub Security Advisory, not an RFC.
