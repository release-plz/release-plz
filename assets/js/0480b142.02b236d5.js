"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[8070],{7208:(e,t,n)=>{n.r(t),n.d(t,{assets:()=>a,contentTitle:()=>r,default:()=>d,frontMatter:()=>i,metadata:()=>o,toc:()=>h});const o=JSON.parse('{"id":"faq","title":"FAQ","description":"What packages does release-plz publish?","source":"@site/docs/faq.md","sourceDirName":".","slug":"/faq","permalink":"/docs/faq","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/faq.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"Semver check","permalink":"/docs/semver-check"},"next":{"title":"Why yet another release tool","permalink":"/docs/why"}}');var s=n(4848),l=n(8453);const i={},r="FAQ",a={},h=[{value:"What packages does release-plz publish?",id:"what-packages-does-release-plz-publish",level:2},{value:"Can I edit the release PR before merging it?",id:"can-i-edit-the-release-pr-before-merging-it",level:2},{value:"Does the changelog include the commits from the whole repo?",id:"does-the-changelog-include-the-commits-from-the-whole-repo",level:2},{value:"What if a commit doesn&#39;t follow the conventional-commits format?",id:"what-if-a-commit-doesnt-follow-the-conventional-commits-format",level:2},{value:"How do I know the branch of the release PR?",id:"how-do-i-know-the-branch-of-the-release-pr",level:2},{value:"Release-plz opens a PR too often",id:"release-plz-opens-a-pr-too-often",level:2},{value:"Release-plz bumped the version in a way I didn&#39;t expect",id:"release-plz-bumped-the-version-in-a-way-i-didnt-expect",level:2}];function c(e){const t={a:"a",code:"code",h1:"h1",h2:"h2",header:"header",li:"li",p:"p",pre:"pre",ul:"ul",...(0,l.R)(),...e.components};return(0,s.jsxs)(s.Fragment,{children:[(0,s.jsx)(t.header,{children:(0,s.jsx)(t.h1,{id:"faq",children:"FAQ"})}),"\n",(0,s.jsx)(t.h2,{id:"what-packages-does-release-plz-publish",children:"What packages does release-plz publish?"}),"\n",(0,s.jsx)(t.p,{children:"Release-plz publishes all packages, except:"}),"\n",(0,s.jsxs)(t.ul,{children:["\n",(0,s.jsxs)(t.li,{children:["packages with ",(0,s.jsx)(t.code,{children:"publish = false"})," in the ",(0,s.jsx)(t.code,{children:"Cargo.toml"}),"."]}),"\n",(0,s.jsxs)(t.li,{children:[(0,s.jsx)(t.a,{href:"https://doc.rust-lang.org/cargo/reference/cargo-targets.html#examples",children:"examples"})," that don't\nspecify the ",(0,s.jsx)(t.a,{href:"https://doc.rust-lang.org/cargo/reference/manifest.html#the-publish-field",children:(0,s.jsx)(t.code,{children:"publish"})}),"\nfield in their ",(0,s.jsx)(t.code,{children:"Cargo.toml"})," file. To publish them, set this field."]}),"\n"]}),"\n",(0,s.jsxs)(t.p,{children:["Even, if a package is not published, release-plz will update its ",(0,s.jsx)(t.code,{children:"Cargo.toml"})," to bump the version of\na local dependency if needed."]}),"\n",(0,s.jsxs)(t.p,{children:["If you want to check which packages release-plz will publish, run\n",(0,s.jsx)(t.code,{children:"release-plz release --dry-run"}),"."]}),"\n",(0,s.jsx)(t.h2,{id:"can-i-edit-the-release-pr-before-merging-it",children:"Can I edit the release PR before merging it?"}),"\n",(0,s.jsx)(t.p,{children:"Yes, you can edit the release PR as you would do with any other PR."}),"\n",(0,s.jsx)(t.p,{children:"Here are some reasons why you might want to edit the release PR:"}),"\n",(0,s.jsxs)(t.ul,{children:["\n",(0,s.jsxs)(t.li,{children:["Edit the changelog: the ",(0,s.jsx)(t.code,{children:"CHANGELOG.md"})," file produced by release-plz is\na good starting point, but you might want to add more details to it.\nRelease-plz populates the corresponding git release description with the new\nchanges of the changelog file.\nNote: you don't need to edit the collabsible changelog in the PR description."]}),"\n",(0,s.jsxs)(t.li,{children:["Edit the version: if you forgot to mark a commit as a\n",(0,s.jsx)(t.a,{href:"https://www.conventionalcommits.org/en/v1.0.0/#commit-message-with-description-and-breaking-change-footer",children:"breaking change"}),",\nor if cargo-semver-checks\n",(0,s.jsx)(t.a,{href:"https://github.com/obi1kenobi/cargo-semver-checks#will-cargo-semver-checks-catch-every-semver-violation",children:"failed"}),"\nto detect a breaking change, you can manually edit the version of the package."]}),"\n"]}),"\n",(0,s.jsx)(t.h2,{id:"does-the-changelog-include-the-commits-from-the-whole-repo",children:"Does the changelog include the commits from the whole repo?"}),"\n",(0,s.jsx)(t.p,{children:"The changelog of each crate includes the commit that changed one of the\nfiles of the crate or one of its dependencies."}),"\n",(0,s.jsx)(t.h2,{id:"what-if-a-commit-doesnt-follow-the-conventional-commits-format",children:"What if a commit doesn't follow the conventional-commits format?"}),"\n",(0,s.jsxs)(t.p,{children:["By default, it will be listed under the section ",(0,s.jsx)(t.code,{children:"### Other"}),".\nYou can customize the changelog format with the\n",(0,s.jsx)(t.a,{href:"/docs/config#the-changelog-section",children:(0,s.jsx)(t.code,{children:"[changelog]"})})," configuration section."]}),"\n",(0,s.jsx)(t.h2,{id:"how-do-i-know-the-branch-of-the-release-pr",children:"How do I know the branch of the release PR?"}),"\n",(0,s.jsx)(t.p,{children:"If you want to commit something to the release-plz pr branch\nafter releaze-plz workflow, you need to know the name of the branch\nof the release PR.\nTo do so, you can:"}),"\n",(0,s.jsxs)(t.ul,{children:["\n",(0,s.jsxs)(t.li,{children:["Query the ",(0,s.jsx)(t.code,{children:"/pulls"})," GitHub\n",(0,s.jsx)(t.a,{href:"https://docs.github.com/en/free-pro-team@latest/rest/pulls/pulls?apiVersion=2022-11-28#list-pull-requests",children:"endpoint"}),".\nFor example, release-plz does it\n",(0,s.jsx)(t.a,{href:"https://github.com/release-plz/release-plz/blob/a92629ed10b8bb42dde426c0f0001aebbb6fa70e/crates/release_plz_core/src/git/backend.rs#L238",children:"here"}),"."]}),"\n",(0,s.jsxs)(t.li,{children:["Use ",(0,s.jsx)(t.code,{children:"git tag | grep release-plz"}),"."]}),"\n"]}),"\n",(0,s.jsxs)(t.p,{children:["If none of these options work for you or you want release-plz to output\nthe branch in the jobs\n",(0,s.jsx)(t.a,{href:"https://docs.github.com/en/actions/using-jobs/defining-outputs-for-jobs",children:"output"}),",\nplease open an issue."]}),"\n",(0,s.jsx)(t.h2,{id:"release-plz-opens-a-pr-too-often",children:"Release-plz opens a PR too often"}),"\n",(0,s.jsx)(t.p,{children:"Release-plz opens a PR when any of the files packaged in the crate changes."}),"\n",(0,s.jsx)(t.p,{children:"To list the files that cargo published to the registry, run:"}),"\n",(0,s.jsx)(t.pre,{children:(0,s.jsx)(t.code,{className:"language-sh",children:"cargo package --list\n"})}),"\n",(0,s.jsxs)(t.p,{children:["To exclude a file from the list (and therefore from the release PR and ",(0,s.jsx)(t.code,{children:"release-plz update"})," changes),\nedit the ",(0,s.jsx)(t.code,{children:"exclude"})," and ",(0,s.jsx)(t.code,{children:"include"}),"\n",(0,s.jsx)(t.a,{href:"https://doc.rust-lang.org/cargo/reference/manifest.html#the-exclude-and-include-fields",children:"fields"}),"\nof the ",(0,s.jsx)(t.code,{children:"Cargo.toml"}),"."]}),"\n",(0,s.jsx)(t.h2,{id:"release-plz-bumped-the-version-in-a-way-i-didnt-expect",children:"Release-plz bumped the version in a way I didn't expect"}),"\n",(0,s.jsxs)(t.p,{children:["Release-plz uses the ",(0,s.jsx)(t.a,{href:"https://crates.io/crates/next_version",children:"next_version"}),"\ncrate to determine the next version.\nPlease read the ",(0,s.jsx)(t.a,{href:"https://docs.rs/next_version/latest/next_version/",children:"documentation"}),",\nand open an issue if it's not clear enough."]})]})}function d(e={}){const{wrapper:t}={...(0,l.R)(),...e.components};return t?(0,s.jsx)(t,{...e,children:(0,s.jsx)(c,{...e})}):c(e)}},8453:(e,t,n)=>{n.d(t,{R:()=>i,x:()=>r});var o=n(6540);const s={},l=o.createContext(s);function i(e){const t=o.useContext(l);return o.useMemo((function(){return"function"==typeof e?e(t):{...t,...e}}),[t,e])}function r(e){let t;return t=e.disableParentContext?"function"==typeof e.components?e.components(s):e.components||s:i(e.components),o.createElement(l.Provider,{value:t},e.children)}}}]);