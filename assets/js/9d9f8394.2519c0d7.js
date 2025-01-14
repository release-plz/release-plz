"use strict";(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[9013],{269:(e,s,n)=>{n.r(s),n.d(s,{assets:()=>a,contentTitle:()=>o,default:()=>d,frontMatter:()=>r,metadata:()=>l,toc:()=>c});const l=JSON.parse('{"id":"troubleshooting","title":"Troubleshooting","description":"Release-plz GitHub action started misbehaving","source":"@site/docs/troubleshooting.md","sourceDirName":".","slug":"/troubleshooting","permalink":"/docs/troubleshooting","draft":false,"unlisted":false,"editUrl":"https://github.com/release-plz/release-plz/tree/main/website/docs/troubleshooting.md","tags":[],"version":"current","frontMatter":{},"sidebar":"tutorialSidebar","previous":{"title":"Why yet another release tool","permalink":"/docs/why"},"next":{"title":"Extra","permalink":"/docs/extra/"}}');var t=n(4848),i=n(8453);const r={},o="Troubleshooting",a={},c=[{value:"Release-plz GitHub action started misbehaving",id:"release-plz-github-action-started-misbehaving",level:2},{value:"<code>release-plz release</code> hangs",id:"release-plz-release-hangs",level:2},{value:"See <code>DEBUG</code> logs",id:"see-debug-logs",level:2}];function h(e){const s={a:"a",blockquote:"blockquote",code:"code",em:"em",h1:"h1",h2:"h2",header:"header",li:"li",p:"p",ul:"ul",...(0,i.R)(),...e.components};return(0,t.jsxs)(t.Fragment,{children:[(0,t.jsx)(s.header,{children:(0,t.jsx)(s.h1,{id:"troubleshooting",children:"Troubleshooting"})}),"\n",(0,t.jsx)(s.h2,{id:"release-plz-github-action-started-misbehaving",children:"Release-plz GitHub action started misbehaving"}),"\n",(0,t.jsxs)(s.blockquote,{children:["\n",(0,t.jsxs)(s.p,{children:["Did your release-plz GitHub action started misbehaving after a ",(0,t.jsx)(s.a,{href:"https://github.com/release-plz/release-plz/releases",children:"Release-plz"}),"\nor ",(0,t.jsx)(s.a,{href:"https://github.com/release-plz/action/releases",children:"GitHub action"})," release?"]}),"\n"]}),"\n",(0,t.jsx)(s.p,{children:"If yes, try to:"}),"\n",(0,t.jsxs)(s.ul,{children:["\n",(0,t.jsxs)(s.li,{children:["\n",(0,t.jsxs)(s.p,{children:[(0,t.jsx)(s.em,{children:"Pin a specific version of the release-plz GitHub action"})," in your workflow file.\nE.g. go from ",(0,t.jsx)(s.code,{children:"release-plz/action@v0.5"})," to ",(0,t.jsx)(s.code,{children:"release-plz/action@v0.5.16"}),".\nDetermine the right version to pin by looking at the previous GitHub Action\n",(0,t.jsx)(s.a,{href:"https://github.com/release-plz/action/releases",children:"releases"})]}),"\n"]}),"\n",(0,t.jsxs)(s.li,{children:["\n",(0,t.jsxs)(s.p,{children:[(0,t.jsx)(s.em,{children:"Pin a specific version of the release-plz"})," in the GitHub action, by specifying the ",(0,t.jsx)(s.code,{children:"version"})," field\nin the GitHub Action ",(0,t.jsx)(s.a,{href:"/docs/github/input",children:"input"}),".\nE.g. ",(0,t.jsx)(s.code,{children:'version: "0.3.70"'}),".\nThe default is the latest version of release-plz.\nDetermine the right version to pin by looking at the previous release-plz\n",(0,t.jsx)(s.a,{href:"https://github.com/release-plz/release-plz/releases",children:"releases"})]}),"\n"]}),"\n"]}),"\n",(0,t.jsxs)(s.p,{children:["Please open an ",(0,t.jsx)(s.a,{href:"https://github.com/release-plz/release-plz/issues",children:"issue"}),", too."]}),"\n",(0,t.jsxs)(s.h2,{id:"release-plz-release-hangs",children:[(0,t.jsx)(s.code,{children:"release-plz release"})," hangs"]}),"\n",(0,t.jsxs)(s.p,{children:["Something similar happened in ",(0,t.jsx)(s.a,{href:"https://github.com/release-plz/release-plz/issues/1015",children:"#1015"}),".\nTry to set a low ",(0,t.jsx)(s.a,{href:"/docs/config#the-publish_timeout-field",children:(0,t.jsx)(s.code,{children:"publish_timeout"})}),"\nin your ",(0,t.jsx)(s.code,{children:"release-plz.toml"})," file to check if release-plz\nis having issues to:"]}),"\n",(0,t.jsxs)(s.ul,{children:["\n",(0,t.jsx)(s.li,{children:"check if a package was published."}),"\n",(0,t.jsx)(s.li,{children:"publish a package."}),"\n"]}),"\n",(0,t.jsxs)(s.h2,{id:"see-debug-logs",children:["See ",(0,t.jsx)(s.code,{children:"DEBUG"})," logs"]}),"\n",(0,t.jsxs)(s.p,{children:["Release-plz uses the ",(0,t.jsx)(s.code,{children:"RUST_LOG"})," environment variable to filter the level of the printed logs.\nBy default, release-plz shows logs at the ",(0,t.jsx)(s.code,{children:"info"})," level, or more severe.\nTo see debug logs, use ",(0,t.jsx)(s.code,{children:"RUST_LOG=debug release-plz"}),".\nIf you want something even more details, use ",(0,t.jsx)(s.code,{children:"RUST_LOG=trace release-plz"})]})]})}function d(e={}){const{wrapper:s}={...(0,i.R)(),...e.components};return s?(0,t.jsx)(s,{...e,children:(0,t.jsx)(h,{...e})}):h(e)}},8453:(e,s,n)=>{n.d(s,{R:()=>r,x:()=>o});var l=n(6540);const t={},i=l.createContext(t);function r(e){const s=l.useContext(i);return l.useMemo((function(){return"function"==typeof e?e(s):{...s,...e}}),[s,e])}function o(e){let s;return s=e.disableParentContext?"function"==typeof e.components?e.components(t):e.components||t:r(e.components),l.createElement(i.Provider,{value:s},e.children)}}}]);