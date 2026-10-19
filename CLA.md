# Contributor Licence Agreement (CLA)

> **Status: the agreements are drafts pending legal review, and CLA enforcement is not yet switched on.**
> External pull requests cannot be merged until ASPEC RACING PTY LTD approves the agreements and enables PactDock for this repository.

## Why TreeDock requires a CLA

ASPEC RACING PTY LTD owns TreeDock and publishes it under the
[PolyForm Noncommercial License 1.0.0](LICENSE.md). ASPEC also offers commercial licences and may relicense its own code in future.

To include a contribution in those editions, ASPEC needs broader rights than the noncommercial licence grants. Every external contribution submitted through a pull request therefore requires a signed CLA.

## The agreements

| Agreement | Use it if… | Text |
|---|---|---|
| ASPEC Individual Contributor Licence Agreement 1.0 | You contribute on your own behalf | [.github/cla/ASPEC-ICLA-1.0.md](.github/cla/ASPEC-ICLA-1.0.md) |
| ASPEC Entity Contributor Licence Agreement 1.0 | You contribute on behalf of an employer or other organisation | [.github/cla/ASPEC-ECLA-1.0.md](.github/cla/ASPEC-ECLA-1.0.md) |

These are ASPEC's own agreements, modelled on the structure of the [Harmony Agreements](https://www.harmonyagreements.org/agreements). They are shared across ASPEC projects and maintained with [PactDock](https://github.com/ASPEC-RACING-PTY-LTD/PactDock). In summary:

- **You keep ownership** of your contributions.
- **The rights ASPEC RACING PTY LTD receives:**
  - A perpetual, worldwide, irrevocable, royalty-free, transferable and sublicensable licence to use, modify, distribute and commercially exploit your contributions.
  - The right to relicense them under any terms, including commercial and proprietary licences, without asking again.
- **A patent licence** and an Australian moral-rights consent.
- **Independent forks** do not have to grant their changes back to ASPEC. The CLA applies only to contributions submitted to TreeDock.

The agreement text is what applies. This summary is not legal advice.

## How signing will work

Signing is handled by **PactDock**, ASPEC's CLA tool, which runs as a GitHub Action on this repository.

1. When you open a pull request, PactDock checks every contributor. That includes the PR author, each commit author and each `Co-authored-by:` identity.
2. If anyone has not signed, the `PactDock CLA` check fails. PactDock then posts a comment with instructions.
3. To sign the individual agreement, post this exact sentence as a **new** comment on the pull request:

   ```text
   I have read and agree to the ASPEC Individual Contributor Licence Agreement version 1.0.
   ```

4. For an organisation, an authorised signatory posts the entity sentence shown in PactDock's comment. A maintainer then verifies their authority before entity coverage becomes active.
5. **Signature records** are kept in a private ASPEC repository. Each record holds your GitHub account ID, the agreement version and hash, your comment and timestamps.

One signature covers all your future contributions to ASPEC projects that use the same agreement version.
