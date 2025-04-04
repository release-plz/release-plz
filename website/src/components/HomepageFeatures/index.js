import React from "react";
import clsx from "clsx";
import styles from "./styles.module.css";

const FeatureList = [
  {
    title: "Changelog generation 📝",
    description: (
      <>
        Release-plz updates your changelogs with{" "}
        <a href="https://git-cliff.org">git-cliff</a> using{" "}
        <a href="https://keepachangelog.com/en/1.0.0/">Keep a changelog</a> format by default.
      </>
    ),
  },
  {
    title: "Version bump ⤴️",
    description: (
      <>
        Release-plz bumps the versions of your crates according to{" "}
        <a href="https://semver.org/">Semantic Versioning</a>, based on{" "}
        <a href="https://www.conventionalcommits.org/">Conventional Commits</a> and API breaking
        changes detected by{" "}
        <a href="https://github.com/obi1kenobi/cargo-semver-checks">cargo-semver-checks</a>.
      </>
    ),
  },
  {
    title: "Release PR 🤖",
    description: (
      <>
        Release-plz opens a PR with the changes to <code>CHANGELOG.md</code>,{" "}
        <code>Cargo.toml</code>{" "}
        and <code>Cargo.lock</code>. When you merge the PR, release-plz creates the tag and the
        release on GitHub/Gitea/GitLab and publishes the crate to crates.io.
      </>
    ),
  },
];

function Feature({ title, description }) {
  return (
    <div className={clsx("col col--4")}>
      <div className="text--center padding-horiz--md">
        <h3>{title}</h3>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures() {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
