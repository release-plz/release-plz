import React from 'react';
import famousProjectsData from '../data/famous-projects.json';
import styles from './styles.module.css';

interface FamousProject {
  name: string;
  url: string;
  stars: number;
  description: string | null;
}

function formatStars(stars: number): string {
  if (stars >= 1000) {
    return `${(stars / 1000).toFixed(1)}k`;
  }
  return stars.toString();
}

export default function FamousProjects(): JSX.Element {
  const projects: FamousProject[] = famousProjectsData;

  return (
    <div className={styles.famousProjects}>
      <ul>
        {projects.map((project) => (
          <li key={project.name}>
            <a href={project.url} target="_blank" rel="noopener noreferrer">
              {project.name}
            </a>
            <span className={styles.projectStars}>
              ⭐ {formatStars(project.stars)}
            </span>
            {project.description && (
              <span className={styles.projectDescription}>
                — {project.description}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}