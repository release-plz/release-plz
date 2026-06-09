import React from 'react';
import Link from '@docusaurus/Link';
import styles from './styles.module.css';

export default function ProjectsCounter(): JSX.Element {
  // This number is updated manually or via a fetch script
  // Approximate the count from GitHub dependents: https://github.com/release-plz/action/network/dependents
  const PROJECTS_COUNT = '1,500+';

  return (
    <div className={styles.projectsCounter}>
      <Link 
        to="https://github.com/release-plz/action/network/dependents"
        className={styles.projectsCounterLink}
      >
        <strong>Used by {PROJECTS_COUNT} Rust projects</strong>
      </Link>
    </div>
  );
}