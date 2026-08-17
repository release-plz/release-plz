import React from "react";
import POPULAR_PROJECTS from "@site/src/data/popular-projects";

/**
 * Renders the most-starred repositories that use release-plz, as a sortable
 * table. The data is generated at build time by `fetch-popular-projects.ts`
 * (see `npm run fetch-popular-projects`).
 */
export default function PopularProjects() {
  return (
    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>Project</th>
          <th>Stars</th>
        </tr>
      </thead>
      <tbody>
        {POPULAR_PROJECTS.map((project, index) => (
          <tr key={project.name}>
            <td>{index + 1}</td>
            <td>
              <a href={project.url} target="_blank" rel="noopener noreferrer">
                {project.name}
              </a>
            </td>
            <td>{project.stars >= 1000
              ? `${(project.stars / 1000).toFixed(project.stars >= 10000 ? 0 : 1)}k`
              : project.stars}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}