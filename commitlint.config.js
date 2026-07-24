module.exports = {
  extends: ['@commitlint/config-conventional'],
  rules: {
    // @semantic-release/git commits the generated CHANGELOG.md notes
    // verbatim as the commit body (.releaserc.json's "message" template),
    // and those notes contain one line per commit with a full commit-link
    // Markdown URL - routinely well over 100 characters. The default
    // config-conventional line-length limits assume a human-typed body, not
    // an auto-generated changelog, and reject semantic-release's own
    // release commit otherwise (confirmed: this blocked the very first
    // release attempt on main). Human-authored commits are unaffected -
    // every other rule (type-enum, subject-case, etc.) still applies.
    'body-max-line-length': [0],
    'footer-max-line-length': [0],
  },
};
