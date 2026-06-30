// Fetch repositories using release-plz action and sort by stars
// This script uses GitHub Search API to find repositories

import https from "https";
import fs from "fs";

interface Repository {
  name: string;
  owner: string;
  full_name: string;
  html_url: string;
  stargazers_count: number;
  description: string | null;
}

interface FamousProject {
  name: string;
  url: string;
  stars: number;
  description: string | null;
}

// GitHub API token (optional, increases rate limit)
// Set GITHUB_TOKEN environment variable or pass as argument
const GITHUB_TOKEN = process.env.GITHUB_TOKEN || "";

// Search query to find repositories using release-plz action
const SEARCH_QUERY = "release-plz%2Faction+path%3A.github%2Fworkflows";

async function fetchFamousProjects() {
  console.log("Fetching repositories using release-plz action...");

  const repositories: Repository[] = [];
  
  // GitHub Search API has a limit of 1000 results per query
  // We need to paginate through results
  const perPage = 100;
  const maxPages = 10; // 1000 results max

  for (let page = 1; page <= maxPages; page++) {
    console.log(`Fetching page ${page}...`);
    
    try {
      const searchUrl = `https://api.github.com/search/code?q=${SEARCH_QUERY}&per_page=${perPage}&page=${page}`;
      
      const searchResult = await fetchGitHubApi(searchUrl);
      
      if (!searchResult.items || searchResult.items.length === 0) {
        console.log(`No more results on page ${page}`);
        break;
      }

      // Get unique repository names from search results
      const repoNames = new Set<string>();
      searchResult.items.forEach((item: any) => {
        repoNames.add(item.repository.full_name);
      });

      // Fetch repository details for each unique repo
      for (const fullName of repoNames) {
        try {
          const repoUrl = `https://api.github.com/repos/${fullName}`;
          const repoDetails = await fetchGitHubApi(repoUrl);
          
          if (repoDetails.stargazers_count > 0) {
            repositories.push({
              name: repoDetails.name,
              owner: repoDetails.owner.login,
              full_name: repoDetails.full_name,
              html_url: repoDetails.html_url,
              stargazers_count: repoDetails.stargazers_count,
              description: repoDetails.description,
            });
          }
        } catch (error) {
          console.error(`Failed to fetch repo ${fullName}:`, error);
        }
      }

      // Check if we've reached the last page
      if (searchResult.items.length < perPage) {
        console.log(`Last page reached (${searchResult.items.length} items)`);
        break;
      }

      // Rate limiting: wait between requests
      await sleep(2000);
    } catch (error) {
      console.error(`Failed to fetch page ${page}:`, error);
      break;
    }
  }

  // Sort repositories by stars (descending)
  repositories.sort((a, b) => b.stargazers_count - a.stargazers_count);

  // Take top 50 projects
  const topProjects: FamousProject[] = repositories.slice(0, 50).map(repo => ({
    name: repo.full_name,
    url: repo.html_url,
    stars: repo.stargazers_count,
    description: repo.description,
  }));

  console.log(`Found ${repositories.length} repositories with stars`);
  console.log(`Top 50 projects will be saved`);

  // Save to JSON file
  const outputPath = "src/data/famous-projects.json";
  fs.writeFileSync(outputPath, JSON.stringify(topProjects, null, 2));
  console.log(`Saved to ${outputPath}`);

  // Also save a summary
  console.log("\nTop 10 projects:");
  topProjects.slice(0, 10).forEach((project, index) => {
    console.log(`${index + 1}. ${project.name} (${formatStars(project.stars)} stars)`);
  });
}

async function fetchGitHubApi(url: string): Promise<any> {
  return new Promise((resolve, reject) => {
    const options = {
      headers: {
        "User-Agent": "release-plz-website-fetcher",
        "Accept": "application/vnd.github.v3+json",
        ...(GITHUB_TOKEN ? { "Authorization": `token ${GITHUB_TOKEN}` } : {}),
      },
    };

    https.get(url, options, (response) => {
      let data = "";

      response.on("data", (chunk) => {
        data += chunk;
      });

      response.on("end", () => {
        if (response.statusCode === 200) {
          try {
            resolve(JSON.parse(data));
          } catch (error) {
            reject(new Error(`Failed to parse JSON: ${error}`));
          }
        } else if (response.statusCode === 403) {
          reject(new Error(`Rate limit exceeded. Set GITHUB_TOKEN environment variable.`));
        } else {
          reject(new Error(`HTTP ${response.statusCode}: ${data}`));
        }
      });
    }).on("error", reject);
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function formatStars(stars: number): string {
  if (stars >= 1000) {
    return `${(stars / 1000).toFixed(1)}k`;
  }
  return stars.toString();
}

// Run the fetcher
fetchFamousProjects().catch(console.error);