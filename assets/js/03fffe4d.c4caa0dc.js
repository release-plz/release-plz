"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[6111],{6034:(e,n,s)=>{s.r(n),s.d(n,{assets:()=>h,contentTitle:()=>i,default:()=>d,frontMatter:()=>t,metadata:()=>r,toc:()=>o});const r=JSON.parse('{"id":"semver-check","title":"Semver check","description":"Release-plz uses cargo-semver-checks","source":"@site/docs/semver-check.md","sourceDirName":".","slug":"/semver-check","permalink":"/docs/semver-check","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/semver-check.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"Tips And Tricks","permalink":"/docs/changelog/tips-and-tricks"},"next":{"title":"FAQ","permalink":"/docs/faq"}}');var c=s(4848),a=s(8453);const t={},i="Semver check",h={},o=[{value:"FAQ",id:"faq",level:2},{value:"What&#39;s an API breaking change?",id:"whats-an-api-breaking-change",level:2},{value:"Will cargo-semver-checks catch every semver violation?",id:"will-cargo-semver-checks-catch-every-semver-violation",level:2},{value:"What happens when release-plz detects API breaking changes?",id:"what-happens-when-release-plz-detects-api-breaking-changes",level:2}];function l(e){const n={a:"a",code:"code",h1:"h1",h2:"h2",header:"header",img:"img",li:"li",p:"p",ul:"ul",...(0,a.R)(),...e.components};return(0,c.jsxs)(c.Fragment,{children:[(0,c.jsx)(n.header,{children:(0,c.jsx)(n.h1,{id:"semver-check",children:"Semver check"})}),"\n",(0,c.jsxs)(n.p,{children:["Release-plz uses ",(0,c.jsx)(n.a,{href:"https://github.com/obi1kenobi/cargo-semver-checks",children:"cargo-semver-checks"}),"\nto check for API breaking changes in libraries."]}),"\n",(0,c.jsxs)(n.p,{children:["The check results are shown in the release Pull Request and in the output of the\n",(0,c.jsx)(n.code,{children:"release-plz update"})," command:"]}),"\n",(0,c.jsxs)(n.ul,{children:["\n",(0,c.jsx)(n.li,{children:"If the check is skipped, release-plz shows nothing. This happens when the package\ndoesn't contain a library."}),"\n",(0,c.jsx)(n.li,{children:'If the check is successful, release-plz shows "(\u2713 API compatible changes)".'}),"\n",(0,c.jsx)(n.li,{children:'If the check failed, release-plz shows "(\u26a0\ufe0f API breaking changes)", with a report\nof what went wrong.'}),"\n"]}),"\n",(0,c.jsx)(n.p,{children:"Example:"}),"\n",(0,c.jsx)(n.p,{children:(0,c.jsx)(n.img,{alt:"pr",src:s(9084).A+"",width:"2298",height:"1466"})}),"\n",(0,c.jsxs)(n.p,{children:["You can configure whether to run ",(0,c.jsx)(n.code,{children:"cargo-semver-checks"})," or not in the\n",(0,c.jsx)(n.a,{href:"/docs/config#the-semver_check-field",children:"configuration file"}),"."]}),"\n",(0,c.jsx)(n.h2,{id:"faq",children:"FAQ"}),"\n",(0,c.jsx)(n.h2,{id:"whats-an-api-breaking-change",children:"What's an API breaking change?"}),"\n",(0,c.jsx)(n.p,{children:"It is a change that makes the new version of your library\nincompatible with the previous one."}),"\n",(0,c.jsx)(n.p,{children:"For example, renaming a public function of your library is an API breaking change,\nbecause the users of your library will have to update their code to use the new name."}),"\n",(0,c.jsx)(n.h2,{id:"will-cargo-semver-checks-catch-every-semver-violation",children:"Will cargo-semver-checks catch every semver violation?"}),"\n",(0,c.jsxs)(n.p,{children:["No, it won't \u2014 not yet! There are many ways to break semver, and ",(0,c.jsx)(n.code,{children:"cargo-semver-checks"}),"\n",(0,c.jsx)(n.a,{href:"https://github.com/obi1kenobi/cargo-semver-checks/issues/5",children:"doesn't yet have lints for all of them"}),".\nYou still need to check for semver violations manually."]}),"\n",(0,c.jsx)(n.h2,{id:"what-happens-when-release-plz-detects-api-breaking-changes",children:"What happens when release-plz detects API breaking changes?"}),"\n",(0,c.jsxs)(n.p,{children:['When release-plz detects API breaking changes, it updates the version of the package\nwith a "major semver Bump". For example, in the image above, you can see that release-plz updated\nthe ',(0,c.jsx)(n.code,{children:"release_plz_core"})," version from ",(0,c.jsx)(n.code,{children:"0.4.21"})," to ",(0,c.jsx)(n.code,{children:"0.5.0"}),".\nIn this way, the users of your library know that the new version contains API breaking\nchanges, and ",(0,c.jsx)(n.code,{children:"cargo update"})," will not update to it automatically."]}),"\n",(0,c.jsxs)(n.p,{children:["You can learn more about semver in the ",(0,c.jsx)(n.a,{href:"https://semver.org/",children:"semver website"}),"\nand in the ",(0,c.jsx)(n.a,{href:"https://doc.rust-lang.org/cargo/reference/semver.html",children:"cargo book"})]})]})}function d(e={}){const{wrapper:n}={...(0,a.R)(),...e.components};return n?(0,c.jsx)(n,{...e,children:(0,c.jsx)(l,{...e})}):l(e)}},9084:(e,n,s)=>{s.d(n,{A:()=>r});const r=s.p+"assets/images/pr-83eb2c4059cd3991cd92b3b43e156baf.png"},8453:(e,n,s)=>{s.d(n,{R:()=>t,x:()=>i});var r=s(6540);const c={},a=r.createContext(c);function t(e){const n=r.useContext(a);return r.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function i(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(c):e.components||c:t(e.components),r.createElement(a.Provider,{value:n},e.children)}}}]);