"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[662],{6263:(e,t,n)=>{n.r(t),n.d(t,{assets:()=>c,contentTitle:()=>r,default:()=>h,frontMatter:()=>o,metadata:()=>a,toc:()=>l});var i=n(5893),s=n(1151);const o={},r="Security",a={id:"github/security",title:"Security",description:"In the following, we'll discuss some security considerations when using the release-plz GitHub",source:"@site/docs/github/security.md",sourceDirName:"github",slug:"/github/security",permalink:"/docs/github/security",draft:!1,unlisted:!1,editUrl:"https://github.com/MarcoIeni/release-plz/tree/main/website/docs/github/security.md",tags:[],version:"current",frontMatter:{},sidebar:"tutorialSidebar",previous:{title:"Advanced Configuration",permalink:"/docs/github/advanced"},next:{title:"Configuration",permalink:"/docs/config"}},c={},l=[{value:"Using latest version",id:"using-latest-version",level:2},{value:"\u26a0\ufe0f Risk: malicious code published on your crates.io crate",id:"\ufe0f-risk-malicious-code-published-on-your-cratesio-crate",level:3},{value:"\u2705 Solution: pin the action version",id:"-solution-pin-the-action-version",level:3}];function u(e){const t={a:"a",code:"code",h1:"h1",h2:"h2",h3:"h3",p:"p",pre:"pre",...(0,s.a)(),...e.components};return(0,i.jsxs)(i.Fragment,{children:[(0,i.jsx)(t.h1,{id:"security",children:"Security"}),"\n",(0,i.jsx)(t.p,{children:"In the following, we'll discuss some security considerations when using the release-plz GitHub\naction and how to mitigate them."}),"\n",(0,i.jsx)(t.h2,{id:"using-latest-version",children:"Using latest version"}),"\n",(0,i.jsx)(t.p,{children:"The examples provided in the documentation use the latest version of the release-plz GitHub action."}),"\n",(0,i.jsxs)(t.p,{children:["For example, the following snippet uses the ",(0,i.jsx)(t.code,{children:"v0.5"})," version of the release-plz GitHub action:"]}),"\n",(0,i.jsx)(t.pre,{children:(0,i.jsx)(t.code,{className:"language-yaml",children:"jobs:\n  release-plz:\n    name: Release-plz\n    runs-on: ubuntu-latest\n    steps:\n      - ...\n      - name: Run release-plz\n        uses: MarcoIeni/release-plz-action@v0.5\n"})}),"\n",(0,i.jsxs)(t.p,{children:[(0,i.jsx)(t.a,{href:"https://github.com/MarcoIeni/release-plz-action/blob/main/.github/workflows/update_main_version.yml",children:"This"}),"\nscript updates this tag to whatever the latest ",(0,i.jsx)(t.code,{children:"0.5.x"})," version is.\nThis means that if the latest version of release-plz is 0.5.34, with ",(0,i.jsx)(t.code,{children:"v0.5"})," you will use that version.\nIf tomorrow, release-plz 0.5.35 is released, you will use that version without the\nneed to update your workflow file."]}),"\n",(0,i.jsx)(t.p,{children:"While this is great for new features and bug fixes, it can also be a security risk."}),"\n",(0,i.jsx)(t.h3,{id:"\ufe0f-risk-malicious-code-published-on-your-cratesio-crate",children:"\u26a0\ufe0f Risk: malicious code published on your crates.io crate"}),"\n",(0,i.jsxs)(t.p,{children:["An attacker who manages to push and tag malicious code to the GitHub action\n",(0,i.jsx)(t.a,{href:"https://github.com/MarcoIeni/release-plz-action",children:"repository"}),"\ncould use your cargo registry token to push malicious code to\nyour crate on crates.io.\nThis means you or your users could download and run the malicious code."]}),"\n",(0,i.jsx)(t.h3,{id:"-solution-pin-the-action-version",children:"\u2705 Solution: pin the action version"}),"\n",(0,i.jsx)(t.p,{children:"To mitigate this risk, you can use a specific version of the release-plz GitHub action.\nBy specifying a commit hash, the action won't be updated automatically."}),"\n",(0,i.jsx)(t.p,{children:"For example:"}),"\n",(0,i.jsx)(t.pre,{children:(0,i.jsx)(t.code,{className:"language-yaml",children:"jobs:\n  release-plz:\n    name: Release-plz\n    runs-on: ubuntu-latest\n    steps:\n      - ...\n      - name: Run release-plz\n        uses: MarcoIeni/release-plz-action@63ab0c2746bedc448370bad4b0b3d536458398b0 # v0.5.50\n\n"})}),"\n",(0,i.jsxs)(t.p,{children:["This is the same approach used in the crates.io\n",(0,i.jsx)(t.a,{href:"https://github.com/rust-lang/crates.io/blob/7e52e11c5ddeb33db70f0000bbcdfb01e9b43b0d/.github/workflows/ci.yml#L30C32-L31C1",children:"repository"}),"."]})]})}function h(e={}){const{wrapper:t}={...(0,s.a)(),...e.components};return t?(0,i.jsx)(t,{...e,children:(0,i.jsx)(u,{...e})}):u(e)}},1151:(e,t,n)=>{n.d(t,{Z:()=>a,a:()=>r});var i=n(7294);const s={},o=i.createContext(s);function r(e){const t=i.useContext(o);return i.useMemo((function(){return"function"==typeof e?e(t):{...t,...e}}),[t,e])}function a(e){let t;return t=e.disableParentContext?"function"==typeof e.components?e.components(s):e.components||s:r(e.components),i.createElement(o.Provider,{value:t},e.children)}}}]);