// Fetch top projects that use release-plz and display them on the website.
//
// This script queries the GitHub API to find repositories that use the
// release-plz GitHub Action, then sorts them by star count and writes the
// result as a static data module so the website can render it without
// runtime API calls.
//
// Usage:
//   GITHUB_TOKEN=... npx tsx ./src/fetch-popular-projects
//
// GITHUB_TOKEN is optional but recommended — without it the code-search
// endpoint is rate-limited to 10 results per minute.

import https from "https";
import fs from "fs";

const OUT = "src/data/popular-projects.tsx";

const GITHUB_TOKEN = process.env.GITHUB_TOKEN || "";

const HEADERS: Record<string, string> = {
  Accept: "application/vnd.github.v3+json",
  "User-Agent": "release-plz-website",
};
if (GITHUB_TOKEN) {
  HEADERS.Authorization = `token ${GITHUB_TOKEN}`;
}

function githubGet(url: string): Promise<any> {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: HEADERS }, (res) => {
        let data = "";
        res.on("data", (chunk) => (data += chunk));
        res.on("end", () => {
          if (res.statusCode === 403) {
            console.warn(`Rate limited (403) on ${url}, returning empty`);
            resolve(null);
            return;
          }
          if (res.statusCode === 302 || res.statusCode === 301) {
            githubGet(res.headers.location!).then(resolve).catch(reject);
            return;
          }
          try {
            resolve(JSON.parse(data));
          } catch {
            reject(new Error(`Failed to parse response from ${url}: ${data.slice(0, 200)}`));
          }
        });
      })
      .on("error", reject);
  });
}

/** Search for repositories that use release-plz-action@ in their workflows. */
async function searchRepos(): Promise<string[]> {
  // Use the dependency search: repositories that depend on the release-plz action
  // via the dependents network. This is more reliable than code search.
  const repos: string[] = [];

  // Query 1: code search for release-plz-action@ in .github/workflows
  const codeQuery =
    encodeURIComponent(
      "path:.github/workflows release-plz-action@"
    );
  const codeUrl = `https://api.github.com/search/code?q=${codeQuery}&per_page=100&sort=indexed`;

  const codeResult = await githubGet(codeUrl);
  if (codeResult && codeResult.items) {
    for (const item of codeResult.items) {
      const fullName = item.repository.full_name;
      if (!repos.includes(fullName)) {
        repos.push(fullName);
      }
    }
  } else {
    console.warn("Code search returned no results (may be rate limited)");
  }

  // Query 2: search for release-plz-action and release-plz-action@
  const actionQuery = encodeURIComponent(
    '"release-plz-action@" path:.github/workflows'
  );
  const actionUrl = `https://api.github.com/search/code?q=${actionQuery}&per_page=100`;

  const actionResult = await githubGet(actionUrl);
  if (actionResult && actionResult.items) {
    for (const item of actionResult.items) {
      const fullName = item.repository.full_name;
      if (!repos.includes(fullName)) {
        repos.push(fullName);
      }
    }
  }

  return repos;
}

/** Get star counts for a batch of repos. */
async function fetchStars(repos: string[]): Promise<{ name: string; stars: number; url: string }[]> {
  const results: { name: string; stars: number; url: string }[] = [];
  const batchSize = 10;

  for (let i = 0; i < repos.length; i += batchSize) {
    const batch = repos.slice(i, i + batchSize);
    const promises = batch.map(async (repo) => {
      try {
        const data = await githubGet(`https://api.github.com/repos/${repo}`);
        if (data && data.stargazers_count != null) {
          return {
            name: repo,
            stars: data.stargazers_count,
            url: data.html_url || `https://github.com/${repo}`,
          };
        }
      } catch (e: any) {
        console.warn(`Failed to fetch stars for ${repo}: ${e.message}`);
      }
      return null;
    });
    const batchResults = await Promise.all(promises);
    for (const r of batchResults) {
      if (r) results.push(r);
    }
    // Small delay between batches to avoid rate limiting
    if (i + batchSize < repos.length) {
      await new Promise((r) => setTimeout(r, 500));
    }
  }

  // Sort by stars descending, take top 40
  results.sort((a, b) => b.stars - a.stars);
  return results.slice(0, 40);
}

async function main() {
  console.log("Fetching popular projects that use release-plz...");

  const repos = await searchRepos();
  console.log(`Found ${repos.length} unique repositories`);

  if (repos.length === 0) {
    console.log(
      "No repositories found (likely rate-limited). Keeping existing data file."
    );
    return;
  }

  const popular = await fetchStars(repos);
  console.log(`Top project: ${popular[0]?.name} with ${popular[0]?.stars} stars`);

  writeDataFile(popular);
}

function writeDataFile(
  projects: { name: string; stars: number; url: string }[]
) {
  const content = `// Auto-generated by fetch-popular-projects.ts -- do not edit manually.
// Re-generate with: npm run fetch-popular-projects

export interface PopularProject {
  name: string;
  stars: number;
  url: string;
}

const POPULAR_PROJECTS: PopularProject[] = ${JSON.stringify(projects, null, 2)};

export default POPULAR_PROJECTS;
`;

  fs.writeFileSync(OUT, content, "utf-8");
  console.log(`Wrote ${projects.length} projects to ${OUT}`);
}

main().catch(console.error);