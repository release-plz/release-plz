import React from "react";
import Tweets from "@site/src/data/feedback";
import clsx from "clsx";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import Layout from "@theme/Layout";
import Heading from "@theme/Heading";
import HomepageFeatures from "@site/src/components/HomepageFeatures";
import ProjectsCounter from "@site/src/components/ProjectsCounter";

import styles from "./index.module.css";
import type { Props as Tweet } from "../components/tweet";
import GetTweet from "../components/tweet";
import Translate from "@docusaurus/Translate";
import Link from "@docusaurus/Link";
import QuickStartCode from "./quickstart_code.mdx";
import MDXContent from "@theme/MDXContent";

function HomepageHeader() {
  return (
    <header className={clsx("hero hero--primary", styles.heroBanner)}>
      <div className="container">
        <h1 className="hero__title">RELEASE-PLZ</h1>
        <img
          alt="Release-plz Logo"
          src="img/robot.jpeg"
          style={{ maxHeight: 500, maxWidth: "100%" }}
        />
        <p className="hero__subtitle">
          Release Rust crates from <b>CI</b> with a <b>Release PR</b>
        </p>
        <div className={styles.buttons}>
          <Link
            style={{ marginRight: 10 }}
            className="button button--secondary button--lg"
            to="/docs"
          >
            Get Started️
          </Link>
        </div>
      </div>
    </header>
  );
}

function QuickStartSection() {
  return (
    <div className={clsx(styles.section, styles.sectionAlt)}>
      <div className="container">
        <Heading as="h2" className={clsx("margin-bottom--lg", "text--center")}>
          It works locally, too 👩‍💻
        </Heading>
        <MDXContent>
          <QuickStartCode />
        </MDXContent>
      </div>
    </div>
  );
}

export default function Home() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout
      title={`${siteConfig.title} Rust crate`}
      description="Release Rust crates from CI with a Release PR"
    >
      <HomepageHeader />
      <main>
        <ProjectsCounter />
        <HomepageFeatures />
      </main>
      <QuickStartSection />
      <TweetsSection />
    </Layout>
  );
}

function TweetsSection() {
  const tweetColumns: Tweet[][] = [[], [], []];
  Tweets.forEach((tweet, i) => tweetColumns[i % 3]!.push(tweet));

  return (
    <div className={clsx(styles.section, styles.sectionAlt)}>
      <div className="container">
        <Heading as="h2" className={clsx("margin-bottom--lg", "text--center")}>
          Loved by many Rustaceans 🦀
        </Heading>
        <div className={clsx("row", styles.tweetsSection)}>
          {tweetColumns.map((tweetItems, i) => (
            <div className="col col--4" key={i}>
              {tweetItems.map((tweet) => (
                <GetTweet {...tweet} key={tweet.url} />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
