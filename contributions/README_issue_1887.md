# Contribution #1887: Display most "famous" projects in the website

**Contribution Number:** 1887
**Student:** Jude Surin
**Issue:** [Display most "famous" projects in the website](https://github.com/release-plz/release-plz/issues/1887)
**Status:** Phase I — In Progress

---

## Why I Chose This Issue

I chose this issue because it sits at the intersection of open-source community-building and web development. Surfacing the most well-known projects that depend on release-plz gives new visitors immediate social proof and makes the documentation site more engaging. It also involves build-time data fetching from the GitHub API — a concrete engineering challenge beyond pure content work.

---

## Understanding the Issue

### Problem Description

The release-plz documentation site's ["Release-plz in the wild"](https://release-plz.ieni.dev/docs/release-plz-in-the-wild) page is maintained manually with no star counts or dynamic counter showing adoption scale. The home page has no indicator of how many projects rely on the GitHub Action.

### Expected Behavior

1. **"In the wild" page** — Auto-generated table of top dependent repositories sorted by stars, built using [`ghtopdep`](https://github.com/github-tooling/ghtopdep) against the [release-plz/action dependents graph](https://github.com/release-plz/action/network/dependents).
2. **Home page counter** — A snippet like *"used by 1,100+ Rust projects"* linking to the dependents graph. The count is fetched at build time and rounded up to the nearest hundred (e.g. 1,134 → "1,100+").
3. **Star counter** *(stretch goal, separate PR)* — Cumulative star count across all top dependent projects on the home page.

### Current Behavior

The "in the wild" page lists ~12 hand-picked projects with no star counts and no automation. The home page has no usage counter.

### Affected Components

- `website/docs/release-plz-in-the-wild.md` — add auto-generated top-dependents table.
- `website/src/pages/index.tsx` — add "used by N+ projects" counter.
- `.github/workflows/deploy-website.yml` — add a pre-build step that runs `ghtopdep` and writes results to a JSON file consumed by the site.

---

## Reproduction Process

### Environment Setup

```bash
# Node/Docusaurus site
node >= 18, npm >= 9

# Python tool for dependents scraping
pip install ghtopdep
export GHTOPDEP_TOKEN=<your-GitHub-PAT-with-public_repo-scope>
```

Docusaurus v3 runs cleanly on Node 20. No environment issues encountered.

### Steps to Reproduce

1. Visit `https://release-plz.ieni.dev/docs/release-plz-in-the-wild` — the list is static with no star counts.
2. Visit the release-plz home page — no "used by N projects" counter exists.
3. Run `ghtopdep https://github.com/release-plz/action/network/dependents --rows 20` — many high-star Rust projects using release-plz are not shown on the site.

### Reproduction Evidence

- **Commit showing reproduction:** *(to be added)*
- **Screenshots/logs:** `ghtopdep` output to be captured once token is configured.
- **Findings:** The current list was assembled by hand; several hundred public repositories depend on the action per the dependency graph links already on the page.

---

## Solution Approach

### Analysis

There is no build-time automation. The data is publicly available — GitHub exposes a dependency graph for `release-plz/action` and `ghtopdep` can scrape and sort it by stars — but nothing pulls it into the Docusaurus build. The counter requires one additional step: parsing the total dependent count and rounding up.

### Proposed Solution

1. Add `scripts/fetch-dependents.mjs` to:
   - Use `ghtopdep` to get the top N dependent repos sorted by stars.
   - Fetch the total dependent count and round up to the nearest hundred.
   - Write `{ topDependents: [...], usedByCount: "1100+" }` to `website/src/data/dependents.json`.
2. Update the Docusaurus site to read `dependents.json`:
   - Render the top-dependents table on the "in the wild" page.
   - Display the rounded counter on the home page.
3. Call the script in CI before `npm run build`.

### Implementation Plan (UMPIRE)

**Understand:** The site needs auto-generated dependent data (top repos by stars + total count) injected at build time.

**Match:** Docusaurus supports importing JSON natively in `.mdx` files, so a pre-build script writing JSON is the lightest-touch approach consistent with existing project patterns.

**Plan:**

1. Create `scripts/fetch-dependents.mjs`:
   - Invoke `ghtopdep` via `child_process.execSync` or use `@octokit/rest` to collect repo names + stars.
   - Extract the total count (exposed by `ghtopdep` as "found N repositories") and compute `Math.floor(total / 100) * 100 + "+"`.
   - Write the results to `website/src/data/dependents.json`.
2. Convert `release-plz-in-the-wild.md` to `.mdx` and import the JSON to render a stars-sorted table.
3. Add the counter to the home page:
   ```jsx
   <a href="https://github.com/release-plz/action/network/dependents">
     used by {usedByCount} Rust projects
   </a>
   ```
4. Add `GH_TOKEN` as a GitHub Actions secret and inject it in the deploy workflow.
5. Update linting/tests as needed.

**Implement:** *(branch link to be added)*

**Review checklist:**
- [ ] Script fails gracefully if rate-limited, with a hard-coded fallback value.
- [ ] No secrets committed to the repo.
- [ ] Docusaurus build passes locally with the generated JSON file.
- [ ] Follows project contribution guidelines (DCO sign-off, conventional commits).

**Evaluate:** Run `npm run build` after the script; verify the counter and table appear in the built site.

---

## Testing Strategy

### Unit Tests

- [ ] `roundUpToHundred(1134)` returns `"1100+"`.
- [ ] `roundUpToHundred(1000)` returns `"1000+"`.
- [ ] Fetch script writes valid JSON with shape `{ topDependents, usedByCount }`.

### Integration Tests

- [ ] Run `scripts/fetch-dependents.mjs` in CI with a scoped GitHub token; assert output file is non-empty.
- [ ] Build the site and assert `"used by"` appears in `index.html`.

### Manual Testing

Build the site locally and verify:
- "In the wild" page shows a table sorted by stars.
- Home page shows the counter linking to the correct URL.
- Offline build falls back to the committed `dependents.json` without crashing.

---

## Implementation Notes

### Week 1 Progress

- Reviewed the live site and confirmed the "in the wild" page is fully manual.
- Confirmed `ghtopdep` has a `--json` output flag suitable for programmatic use.
- Confirmed the site is Docusaurus v3 and JSON imports work natively in `.mdx`.
- Drafted this README and the implementation plan.

### Code Changes

- **Files to modify:** `website/docs/release-plz-in-the-wild.md` → `.mdx`, `website/src/pages/index.tsx`, `.github/workflows/deploy-website.yml`
- **Files to add:** `scripts/fetch-dependents.mjs`, `website/src/data/dependents.json`
- **Key commits:** *(to be added)*
- **Approach decision:** A pre-build script keeps the change minimal and lets maintainers run it locally without needing to understand Docusaurus plugin internals.

---

## Pull Request

**PR Link:** *(to be added)*

**PR Description (draft):**

> **Add auto-generated "used by N+ projects" counter and top-dependents table**
>
> Closes #1887.
>
> - Adds `scripts/fetch-dependents.mjs` to collect top dependent repos (sorted by stars) and total count from the [dependents graph](https://github.com/release-plz/action/network/dependents).
> - Writes results to `website/src/data/dependents.json` at build time.
> - Updates `release-plz-in-the-wild.mdx` to render the auto-generated table.
> - Adds a "used by 1,100+ Rust projects" counter to the home page.
> - Injects `GH_TOKEN` in the deploy workflow for the fetch step.

**Maintainer Feedback:** *(to be added)*

---

## Learnings & Reflections

### Technical Skills Gained

- Using `ghtopdep` to extract and rank GitHub dependent repositories programmatically.
- Docusaurus build pipeline customization (pre-build scripts, JSON data imports in MDX).
- GitHub API rate limits and token scoping for public repository access.
- Formatting large numbers for user-friendly display.

### Challenges Overcome

*(to be filled as work progresses)*

### What I'd Do Differently Next Time

*(to be filled after PR review)*

---

## Resources Used

- [release-plz/release-plz issue #1887](https://github.com/release-plz/release-plz/issues/1887)
- [ghtopdep GitHub repo](https://github.com/github-tooling/ghtopdep) — CLI tool for sorting dependents by stars.
- [release-plz/action network dependents](https://github.com/release-plz/action/network/dependents)
- [release-plz "in the wild" page](https://release-plz.ieni.dev/docs/release-plz-in-the-wild)
- [Docusaurus v3 — Using MDX](https://docusaurus.io/docs/markdown-features/react)
- [GitHub REST API docs](https://docs.github.com/en/rest)
