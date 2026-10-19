import{vn as e}from"./chunk-runtime-core.esm-bundler-DrnlzXx3.js";import{r as t}from"./chunk-VDropdown-BpR51Q5I.js";import{a as n,t as r}from"./chunk-gql-client-CXWvSHcO.js";import{At as i,Ct as a,Dt as o,Et as s,Mt as c,Nt as l,Pt as u,Tt as d,jt as f,kt as p,wt as m}from"./chunk-query--k5TGwaW.js";function ee(i,a=!0){let o=e(!1),s=[],c=[];async function l(e){o.value=!0;try{let o=await n(i.document,e,{dedupe:!1});if(o.errors?.length){let e=o.errors[0].message;a&&t.emit(`toast`,e);let n=new r(e);for(let e of c)e(n);return}for(let e of s)e(o);return o}catch(e){if(e instanceof r&&e.cancelled)return;let n=e instanceof r?e.message:`network_error`;a&&t.emit(`toast`,n);for(let t of c)t(e);return}finally{o.value=!1}}function u(e){return s.push(e),{off:()=>{let t=s.indexOf(e);t>=0&&s.splice(t,1)}}}function d(e){return c.push(e),{off:()=>{let t=c.indexOf(e);t>=0&&c.splice(t,1)}}}return{mutate:l,loading:o,onDone:u,onError:d}}async function te(e,t){return await e(t)!=null}var h=`
  mutation {
    clearAppLogs
  }
`,g=`
  mutation setUserPref($key: String!, $value: JSON!) {
    setUserPref(key: $key, value: $value)
  }
`,_=`
  mutation updateDeviceName($name: String!) {
    updateDeviceName(name: $name)
  }
`,v=`
  mutation forwardChatItems($id: ID!, $targets: [String!]!) {
    forwardChatItems(id: $id, targets: $targets) {
      ...ChatItemFragment
    }
  }
  ${o}
`,y=`
  mutation sendChatItem($target: String!, $content: String!) {
    sendChatItem(target: $target, content: $content) {
      ...ChatItemFragment
    }
  }
  ${o}
`,b=`
  mutation deleteChatItem($id: ID!) {
    deleteChatItem(id: $id)
  }
`,x=`
  mutation deleteChatItems($query: String!) {
    deleteChatItems(query: $query) {
      affectedCount
    }
  }
`,S=`
  mutation retryChatItem($id: ID!) {
    retryChatItem(id: $id) {
      ...ChatItemFragment
    }
  }
  ${o}
`,C=`
  mutation createChatChannel($name: String!) {
    createChatChannel(name: $name) {
      ...ChatChannelFragment
    }
  }
  ${s}
`,w=`
  mutation updateChatChannel($id: ID!, $name: String!) {
    updateChatChannel(id: $id, name: $name) {
      ...ChatChannelFragment
    }
  }
  ${s}
`,T=`
  mutation deleteChatChannel($id: ID!) {
    deleteChatChannel(id: $id)
  }
`,E=`
  mutation deletePeer($id: ID!) {
    deletePeer(id: $id)
  }
`,D=`
  mutation unpairPeer($id: ID!) {
    unpairPeer(id: $id)
  }
`,O=`
  mutation leaveChatChannel($id: ID!) {
    leaveChatChannel(id: $id)
  }
`,k=`
  mutation addChatChannelMember($id: ID!, $peerId: ID!) {
    addChatChannelMember(id: $id, peerId: $peerId) {
      ...ChatChannelFragment
    }
  }
  ${s}
`,A=`
  mutation removeChatChannelMember($id: ID!, $peerId: ID!) {
    removeChatChannelMember(id: $id, peerId: $peerId) {
      ...ChatChannelFragment
    }
  }
  ${s}
`,j=`
  mutation acceptChatChannelInvite($id: ID!) {
    acceptChatChannelInvite(id: $id)
  }
`,M=`
  mutation declineChatChannelInvite($id: ID!) {
    declineChatChannelInvite(id: $id)
  }
`,N=`
  mutation createDir($path: String!) {
    createDir(path: $path) {
      ...FileFragment
    }
  }
  ${c}
`,P=`
  mutation writeTextFile($path: String!, $content: String!, $overwrite: Boolean!) {
    writeTextFile(path: $path, content: $content, overwrite: $overwrite) {
      ...FileFragment
    }
  }
  ${c}
`,F=`
  mutation renameFile($path: String!, $name: String!) {
    renameFile(path: $path, name: $name)
  }
`,I=`
  mutation copyFile($src: String!, $dst: String!, $overwrite: Boolean!) {
    copyFile(src: $src, dst: $dst, overwrite: $overwrite)
  }
`,L=`
  mutation moveFile($src: String!, $dst: String!, $overwrite: Boolean!) {
    moveFile(src: $src, dst: $dst, overwrite: $overwrite)
  }
`,R=`
  mutation reportAudioPlayback($path: String!, $revision: Long!, $positionMs: Long!, $started: Boolean!) {
    reportAudioPlayback(path: $path, revision: $revision, positionMs: $positionMs, started: $started)
  }
`,z=`
  mutation playAudio($mediaId: ID!) {
    playAudio(mediaId: $mediaId) {
      ...AudioItemFragment
    }
  }
  ${a}
`,B=`
  mutation enqueueAndPlayAudio($mediaId: ID!, $query: String!) {
    addAudioToQueue(query: $query)
    playAudio(mediaId: $mediaId) {
      ...AudioItemFragment
    }
  }
  ${a}
`,V=`
  mutation updateAudioPlayMode($mode: MediaPlayMode!) {
    updateAudioPlayMode(mode: $mode)
  }
`,H=`
  mutation removeAudioFromQueue($mediaId: ID!) {
    removeAudioFromQueue(mediaId: $mediaId)
  }
`,U=`
  mutation addAudioToQueue($query: String!) {
    addAudioToQueue(query: $query)
  }
`,W=`
  mutation clearAudioQueue {
    clearAudioQueue
  }
`,G=`
  mutation reorderAudioQueue($mediaIds: [ID!]!) {
    reorderAudioQueue(mediaIds: $mediaIds)
  }
`,K=`
  mutation deleteMediaItems($type: MediaDataType!, $query: String!) {
    deleteMediaItems(type: $type, query: $query) {
      affectedCount
    }
  }
`,q=`
  mutation trashMediaItems($type: MediaDataType!, $query: String!) {
    trashMediaItems(type: $type, query: $query) {
      affectedCount
    }
  }
`,J=`
  mutation restoreMediaItems($type: MediaDataType!, $query: String!) {
    restoreMediaItems(type: $type, query: $query) {
      affectedCount
    }
  }
`,Y=`
  mutation moveMediaItems($type: MediaDataType!, $query: String!, $destDir: String!) {
    moveMediaItems(type: $type, query: $query, destDir: $destDir) {
      affectedCount
    }
  }
`,X=`
  mutation trashSms($query: String!) {
    trashSms(query: $query) {
      affectedCount
    }
  }
`,Z=`
  mutation restoreSms($query: String!) {
    restoreSms(query: $query) {
      affectedCount
    }
  }
`,Q=`
  mutation deleteSms($query: String!) {
    deleteSms(query: $query) {
      affectedCount
    }
  }
`,ne=`
  mutation removeFromTags($type: DataType!, $tagIds: [ID!]!, $query: String!) {
    removeFromTags(type: $type, tagIds: $tagIds, query: $query)
  }
`,re=`
  mutation addToTags($type: DataType!, $tagIds: [ID!]!, $query: String!) {
    addToTags(type: $type, tagIds: $tagIds, query: $query)
  }
`,ie=`
  mutation updateTagRelations($type: DataType!, $keys: [String!]!, $addTagIds: [ID!]!, $removeTagIds: [ID!]!) {
    updateTagRelations(type: $type, keys: $keys, addTagIds: $addTagIds, removeTagIds: $removeTagIds)
  }
`,ae=`
  mutation createTag($type: DataType!, $name: String!) {
    createTag(type: $type, name: $name) {
      ...TagFragment
    }
  }
  ${u}
`,oe=`
  mutation updateTag($id: ID!, $name: String!) {
    updateTag(id: $id, name: $name) {
      ...TagFragment
    }
  }
  ${u}
`,se=`
  mutation deleteTag($id: ID!) {
    deleteTag(id: $id)
  }
`,ce=`
  mutation addFavoriteFolder($rootPath: String!, $fullPath: String!) {
    addFavoriteFolder(rootPath: $rootPath, fullPath: $fullPath) {
      rootPath
      fullPath
    }
  }
`,le=`
  mutation removeFavoriteFolder($fullPath: String!) {
    removeFavoriteFolder(fullPath: $fullPath) {
      rootPath
      fullPath
      alias
    }
  }
`,ue=`
  mutation setFavoriteFolderAlias($fullPath: String!, $alias: String!) {
    setFavoriteFolderAlias(fullPath: $fullPath, alias: $alias) {
      rootPath
      fullPath
      alias
    }
  }
`,de=`
  mutation createNote($input: NoteInput!) {
    createNote(input: $input) {
      ...NoteFragment
    }
  }
  ${l}
`,fe=`
  mutation updateNote($id: ID!, $input: NoteInput!) {
    updateNote(id: $id, input: $input) {
      ...NoteFragment
    }
  }
  ${l}
`,pe=`
  mutation deleteNotes($query: String!) {
    deleteNotes(query: $query) {
      affectedCount
    }
  }
`,me=`
  mutation trashNotes($query: String!) {
    trashNotes(query: $query) {
      affectedCount
    }
  }
`,he=`
  mutation restoreNotes($query: String!) {
    restoreNotes(query: $query) {
      affectedCount
    }
  }
`,ge=`
  mutation deleteFeedEntries($query: String!) {
    deleteFeedEntries(query: $query) {
      affectedCount
    }
  }
`,_e=`
  mutation deleteCalls($query: String!) {
    deleteCalls(query: $query) {
      affectedCount
    }
  }
`,ve=`
  mutation deleteContacts($query: String!) {
    deleteContacts(query: $query) {
      affectedCount
    }
  }
`,ye=`
  mutation createFeed($url: String!, $fetchContent: Boolean!) {
    createFeed(url: $url, fetchContent: $fetchContent) {
      ...FeedFragment
    }
  }
  ${f}
`,be=`
  mutation importFeeds($content: String!) {
    importFeeds(content: $content)
  }
`,xe=`
  mutation exportFeeds {
    exportFeeds
  }
`,Se=`
  mutation exportNotes($query: String!) {
    exportNotes(query: $query)
  }
`,Ce=`
  mutation relaunchApp {
    relaunchApp
  }
`,we=`
  mutation openAccessibilitySettings {
    openAccessibilitySettings
  }
`,Te=`
  mutation openWebSettings($feature: WebSettingsFeature) {
    openWebSettings(feature: $feature)
  }
`,Ee=`
  mutation deleteFeed($id: ID!) {
    deleteFeed(id: $id)
  }
`,De=`
  mutation updateFeed($id: ID!, $name: String!, $fetchContent: Boolean!) {
    updateFeed(id: $id, name: $name, fetchContent: $fetchContent) {
      ...FeedFragment
    }
  }
  ${f}
`,Oe=`
  mutation syncFeeds($id: ID) {
    syncFeeds(id: $id)
  }
`,ke=`
  mutation syncFeedEntryContent($id: ID!) {
    syncFeedEntryContent(id: $id) {
      ...FeedEntryFragment
      feed {
        ...FeedFragment
      }
    }
  }
  ${f}
  ${i}
`,Ae=`
  mutation call($number: String!, $showDialer: Boolean!) {
    call(number: $number, showDialer: $showDialer)
  }
`,je=`
  mutation setClipboard($text: String!) {
    setClipboard(text: $text)
  }
`,Me=`
  mutation sendSms($number: String!, $body: String!, $subscriptionId: Int!) {
    sendSms(number: $number, body: $body, subscriptionId: $subscriptionId)
  }
`,Ne=`
  mutation sendSms($number: String!, $body: String!, $subscriptionId: Int!, $requestId: String!) {
    sendSms(number: $number, body: $body, subscriptionId: $subscriptionId, requestId: $requestId)
  }
`,Pe=`
  mutation archiveSmsConversation($id: String!) {
    archiveSmsConversation(id: $id)
  }
`,Fe=`
  mutation unarchiveSmsConversation($id: String!) {
    unarchiveSmsConversation(id: $id)
  }
`,Ie=`
  mutation sendMms($number: String!, $body: String!, $attachmentPaths: [String!]!, $threadId: ID!) {
    sendMms(number: $number, body: $body, attachmentPaths: $attachmentPaths, threadId: $threadId)
  }
`,Le=`
  mutation uninstallPackages($id: ID!) {
    uninstallPackages(ids: [$id])
  }
`,Re=`
  mutation installPackage($path: String!) {
    installPackage(path: $path) {
      id
      updatedAt
      isNew
    }
  }
`,ze=`
  mutation startScreenMirror($audio: Boolean!) {
    startScreenMirror(audio: $audio)
  }
`,Be=`
  mutation requestScreenMirrorAudio {
    requestScreenMirrorAudio
  }
`,Ve=`
  mutation stopScreenMirror {
    stopScreenMirror
  }
`,He=`
  mutation setTempValue($key: String!, $value: String!) {
    setTempValue(key: $key, value: $value) {
      key
      value
    }
  }
`,Ue=`
  mutation deleteNotifications($ids: [ID!]!) {
    deleteNotifications(ids: $ids) {
      affectedCount
    }
  }
`,We=`
  mutation replyNotification($id: ID!, $actionIndex: Int!, $text: String!) {
    replyNotification(id: $id, actionIndex: $actionIndex, text: $text)
  }
`,Ge=`
  mutation deleteClipboardItems($query: String!) {
    deleteClipboardItems(query: $query) {
      affectedCount
    }
  }
`,Ke=`
  mutation updateScreenMirrorQuality($mode: ScreenMirrorMode!) {
    updateScreenMirrorQuality(mode: $mode)
  }
`,qe=`
  mutation saveFeedEntriesToNotes($query: String!) {
    saveFeedEntriesToNotes(query: $query)
  }
`,Je=`
  mutation mergeChunks($fileId: String!, $totalChunks: Int!, $path: String!, $replace: Boolean!, $totalSize: Long!) {
    mergeChunks(fileId: $fileId, totalChunks: $totalChunks, path: $path, replace: $replace, totalSize: $totalSize) {
      status
      value
      mergedSize
      error
    }
  }
`,Ye=`
  mutation mergeAppFileChunks($fileId: String!, $totalChunks: Int!, $fileName: String!, $totalSize: Long!) {
    mergeAppFileChunks(fileId: $fileId, totalChunks: $totalChunks, fileName: $fileName, totalSize: $totalSize) {
      status
      value
      mergedSize
      error
    }
  }
`,Xe=`
  mutation deleteChunks($fileId: String!) {
    deleteChunks(fileId: $fileId)
  }
`,Ze=`
  mutation startPomodoro($durationSec: Int!) {
    startPomodoro(durationSec: $durationSec)
  }
`,Qe=`
  mutation stopPomodoro {
    stopPomodoro
  }
`,$e=`
  mutation pausePomodoro {
    pausePomodoro
  }
`,et=`
  mutation addBookmarks($urls: [String!]!, $groupId: ID!) {
    addBookmarks(urls: $urls, groupId: $groupId) {
      ...BookmarkFragment
    }
  }
  ${m}
`,tt=`
  mutation updateBookmark($id: ID!, $input: BookmarkInput!) {
    updateBookmark(id: $id, input: $input) {
      ...BookmarkFragment
    }
  }
  ${m}
`,nt=`
  mutation deleteBookmarks($ids: [ID!]!) {
    deleteBookmarks(ids: $ids) {
      affectedCount
    }
  }
`,rt=`
  mutation recordBookmarkClick($id: ID!) {
    recordBookmarkClick(id: $id)
  }
`,it=`
  mutation createBookmarkGroup($name: String!) {
    createBookmarkGroup(name: $name) {
      ...BookmarkGroupFragment
    }
  }
  ${d}
`,at=`
  mutation updateBookmarkGroup($id: ID!, $name: String!, $collapsed: Boolean!, $sortOrder: Int!) {
    updateBookmarkGroup(id: $id, name: $name, collapsed: $collapsed, sortOrder: $sortOrder) {
      ...BookmarkGroupFragment
    }
  }
  ${d}
`,ot=`
  mutation deleteBookmarkGroup($id: ID!) {
    deleteBookmarkGroup(id: $id)
  }
`,st=`
  mutation deleteFiles($paths: [String!]!) {
    deleteFiles(paths: $paths) {
      affectedCount
    }
  }
`,ct=`
  mutation { importImageSearchModel }
`,lt=`
  mutation { enableImageSearch }
`,ut=`
  mutation { disableImageSearch }
`,dt=`
  mutation { cancelImageModelDownload }
`,ft=`
  mutation startImageIndex($force: Boolean) {
    startImageIndex(force: $force)
  }
`,pt=`
  mutation { cancelImageIndex }
`,mt=`
  mutation createContact($input: ContactInput!) {
    createContact(input: $input) {
      ...ContactFragment
    }
  }
  ${p}
`,ht=`
  mutation updateContact($id: ID!, $input: ContactInput!) {
    updateContact(id: $id, input: $input) {
      ...ContactFragment
    }
  }
  ${p}
`,gt=`
  mutation DeleteNote($query: String!) {
    deleteNotes(query: $query) {
      affectedCount
    }
  }
`,_t=`
  mutation deleteFeedEntry($query: String!) {
    deleteFeedEntries(query: $query) {
      affectedCount
    }
  }
`,vt=`
  mutation removeUserPref($key: String!) {
    removeUserPref(key: $key)
  }
`,yt=`
  mutation DeleteDbTableRows($table: String!, $ids: [String!]!) {
    deleteDbTableRows(table: $table, ids: $ids)
  }
`,bt=`
  mutation AcquireDiscovery($token: ID!) {
    acquireDiscovery(token: $token)
  }
`,xt=`
  mutation ReleaseDiscovery($token: ID!) {
    releaseDiscovery(token: $token)
  }
`,St=`
  mutation pairDevice($input: PairingDeviceInput!) {
    pairDevice(input: $input) { deviceId generation mode phase error }
  }
`,Ct=`
  mutation cancelPairing($generation: String!) {
    cancelPairing(generation: $generation)
  }
`,wt=`
  mutation respondToPairing($input: PairingRequestInput!, $accepted: Boolean!) {
    respondToPairing(input: $input, accepted: $accepted)
  }
`,Tt=`
  mutation downloadPeerFile($messageId: ID!, $peerId: ID!) {
    downloadPeerFile(messageId: $messageId, peerId: $peerId)
  }
`,Et=`
  mutation pauseDownload($messageId: ID!) {
    pauseDownload(messageId: $messageId)
  }
`,$=`
  mutation resumeDownload($messageId: ID!, $peerId: ID!) {
    resumeDownload(messageId: $messageId, peerId: $peerId)
  }
`,Dt=`
  mutation retryDownload($messageId: ID!, $peerId: ID!) {
    retryDownload(messageId: $messageId, peerId: $peerId)
  }
`,Ot=`
  mutation pauseMediaScan { pauseMediaScan }
`,kt=`
  mutation resumeMediaScan { resumeMediaScan }
`,At=`
  mutation stopMediaScan { stopMediaScan }
`,jt=`
  mutation rebuildMediaIndex($root: String!) { rebuildMediaIndex(root: $root) }
`,Mt=`
  mutation formatDisk($path: String!) {
    formatDisk(path: $path)
  }
`,Nt=`
  mutation setSambaSettings($input: SambaSettingsInput!) {
    setSambaSettings(input: $input)
  }
`,Pt=`
  mutation setSambaUserPassword($password: String!) {
    setSambaUserPassword(password: $password)
  }
`,Ft=`
  mutation advanceAudioSelection($action: AudioSelectionAction!, $expectedPath: String!, $expectedRevision: Long!) {
    advanceAudioSelection(action: $action, expectedPath: $expectedPath, expectedRevision: $expectedRevision) {
      track { ...AudioItemFragment }
      path
      positionMs
      revision
    }
  }
  ${a}
`;export{v as $,At as $t,x as A,wt as At,gt as B,y as Bt,ae as C,ne as Ct,_e as D,We as Dt,nt as E,G as Et,ge as F,kt as Ft,se as G,ue as Gt,Ue as H,Me as Ht,_t as I,S as It,lt as J,He as Jt,ut as K,Nt as Kt,Ee as L,Dt as Lt,Ge as M,he as Mt,ve as N,Z as Nt,T as O,R as Ot,yt as P,$ as Pt,Mt as Q,ze as Qt,st as R,te as Rt,de as S,le as St,ot as T,F as Tt,E as U,Ne as Ut,pe as V,Ie as Vt,Q as W,je as Wt,xe as X,ft as Xt,B as Y,g as Yt,Se as Z,Ze as Zt,it as _,fe as _n,rt as _t,k as a,me as an,Ye as at,N as b,ie as bn,H as bt,Ft as c,Le as cn,Y as ct,pt as d,tt as dn,St as dt,Qe as en,be as et,dt as f,at as fn,Et as ft,I as g,De as gn,jt as gt,W as h,_ as hn,z as ht,et as i,q as in,O as it,Xe as j,J as jt,b as k,Be as kt,Pe as l,D as ln,we as lt,h as m,ht as mn,$e as mt,bt as n,ke as nn,ee as nt,ce as o,X as on,Je as ot,Ct as p,w as pn,Ot as pt,Tt as q,Pt as qt,U as r,Oe as rn,Re as rt,re as s,Fe as sn,L as st,j as t,Ve as tn,ct as tt,Ae as u,V as un,Te as ut,C as v,Ke as vn,Ce as vt,M as w,vt as wt,ye as x,P as xn,A as xt,mt as y,oe as yn,xt as yt,K as z,qe as zt};