import React from 'react';

import type {Props as Tweet} from '../components/tweet';

export type TweetItem = Tweet & {
  showOnHomepage: boolean;
};

const TWEETS: TweetItem[] = [
  {
    url: 'https://www.linkedin.com/posts/nicholas-matsakis-615614_crates-io-now-supports-setting-up-trusted-activity-7397955814906363904-5SJJ',
    handle: 'nikomatsakis',
    name: 'Niko Matsakis',
    date: 'Nov 22, 2025',
    content: (
      <>
        Between release-plz and trusted publishing and cargo-semver-checks we are getting to a very cool place. My dream is that you get it all setup for free with "cargo new".
      </>
    ),
    showOnHomepage: true,
    githubUsername: 'nikomatsakis',
  },
  {
    url: 'https://hachyderm.io/@hds/112870965641243862',
    handle: 'hds',
    name: 'Hayden Stainsby',
    date: 'Jul 29, 2024',
    content: (
      <>
        Release-plz makes releasing tokio-console super easy.
      </>
    ),
    showOnHomepage: true,
    githubUsername: 'hds',
  },
  {
    url: 'https://twitter.com/algo_luca/status/1781977925299908816',
    handle: 'algo_luca',
    name: 'Luca Palmieri',
    date: 'Apr 21, 2024',
    content: (
      <>
        God bless @MarcoIeni for release-plz.
      </>
    ),
    showOnHomepage: true,
    githubUsername: 'LukeMathWalker',
  },
  {
    url: 'https://fosdem.org/2024/schedule/event/fosdem-2024-2682-semver-in-the-rust-ecosystem-breakage-tooling-and-edge-cases/',
    handle: 'PredragGruevski',
    name: 'Predrag Gruevski',
    date: 'Feb 3, 2024',
    content: (
      <>
        Release-plz will automatically run cargo-semver-checks as part of the release process. If you are in the market for a good release manager, you should check this one out; it's awesome.
      </>
    ),
    showOnHomepage: true,
    githubUsername: 'obi1kenobi',
  },
  {
    url: 'https://github.com/rust-lang/libc/issues/3350#issuecomment-1746436699',
    handle: 'fasterthanlime',
    name: 'Amos Wenger',
    date: 'Oct 4, 2023',
    content: (
      <>
        As far as I can tell, release-plz (an actual Rust executable running in CI, comes with its own GitHub Action) is best-of-class.
      </>
    ),
    showOnHomepage: true,
    githubUsername: 'fasterthanlime',
  },
  {
    url: 'https://www.reddit.com/r/rust/comments/13he55f/comment/jkckcx9/?utm_source=share&utm_medium=web2x&context=3',
    handle: 'XAMPPRocky',
    name: 'Erin Power',
    date: 'May 16, 2023',
    content: (
      <>
        This is a great project to contribute to, it's so incredibly valuable for other maintainers like myself to be able to automate project releases and provide a better experience for our contributors.
      </>
    ),
    showOnHomepage: true,
    githubUsername: 'XAMPPRocky',
  },
];

export default TWEETS;
