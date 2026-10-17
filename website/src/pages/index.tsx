import React, { useEffect, useRef } from "react";
import Tweets from "@site/src/data/feedback";
import clsx from "clsx";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import Layout from "@theme/Layout";
import Heading from "@theme/Heading";
import HomepageFeatures from "@site/src/components/HomepageFeatures";

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
        <HeroVideo />
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

function HeroVideo() {
  const videoRef = useRef<HTMLVideoElement>(null);

  // Autoplay from JS instead of using the `autoplay` attribute, so that:
  // - users who prefer reduced motion only see the poster until they press play.
  // - the video doesn't compete for bandwidth with the page resources.
  useEffect(() => {
    const video = videoRef.current;
    if (!video || window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return;
    }
    // Browsers only allow autoplay of muted videos.
    video.muted = true;
    // Autoplay can still be blocked (e.g. in low power mode). Users can press play.
    video.play().catch(() => {});
  }, []);

  return (
    <div className={styles.heroVideo}>
      <video
        ref={videoRef}
        aria-label="Release-plz explainer video"
        controls
        loop
        muted
        playsInline
        preload="none"
        poster="img/release-plz-video-poster.webp"
        width={1920}
        height={1080}
      >
        <source src="video/release-plz-av1.mp4" type='video/mp4; codecs="av01.0.08M.08"' />
        <source src="video/release-plz-h264.mp4" type="video/mp4" />
      </video>
      <details className={styles.transcript}>
        <summary>Video transcript</summary>
        <ol>
          <li>Release-plz: release Rust crates from CI with a Release PR.</li>
          <li>
            The old way, releasing by hand: bump the version in Cargo.toml, write the changelog, run
            cargo publish, create and push a git tag, draft the GitHub release. Tedious.
            Error-prone.
          </li>
          <li>Step 1: merge to main as usual. Release-plz opens a Release PR.</li>
          <li>
            Step 2: review the Release PR. It contains the next version, computed from conventional
            commits; the changelog, written by git-cliff; and the breaking changes, found by
            cargo-semver-checks.
          </li>
          <li>Step 3: happy with it? Merge.</li>
          <li>
            Step 4: release-plz ships it: git tag, crates.io publish, and release notes on GitHub,
            Gitea or GitLab. Straight from CI. No terminal needed.
          </li>
          <li>
            Built for Rust, batteries included: workspaces (many crates, one PR), zero config (reads
            your Cargo.toml), any registry (crates.io or your own), semver checks (no surprise
            breakage), changelogs (powered by git-cliff), runs anywhere (GitHub Action or CLI).
          </li>
          <li>Release-plz. Stop releasing by hand.</li>
        </ol>
      </details>
    </div>
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
        <HomepageFeatures />
      </main>
      <QuickStartSection />
      <SponsorsSection />
      <TweetsSection />
    </Layout>
  );
}

function SponsorsSection() {
  return (
    <div className={styles.section}>
      <div className="container text--center">
        <Heading as="h2" className="margin-bottom--lg">
          Sponsored by 🤝
        </Heading>
        <a href="https://encore.dev/?utm_source=release-plz&utm_medium=sponsorship&utm_content=website">
          <img className={styles.sponsorLogo} alt="Encore" src="img/sponsors/encore.svg" />
        </a>
        <p className="margin-top--lg margin-bottom--none">
          Want to support Release-plz too?{" "}
          <a href="https://github.com/sponsors/release-plz">Become a sponsor</a>. ❤️
        </p>
      </div>
    </div>
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
