import{Ht as e,L as t,nt as n,ot as r,tt as i,vn as a}from"./chunk-runtime-core.esm-bundler-DrnlzXx3.js";import{a as o,t as s}from"./chunk-gql-client-BUbM3yCl.js";var c=`
  fragment TagFragment on Tag {
    id
    name
    count
  }
`,l=`
  fragment TagSubFragment on Tag {
    id
    name
  }
`,u=`
  fragment AudioItemFragment on AudioItem {
    mediaId
    title
    artist
    path
    durationMs
  }
`,d=`
  fragment AppFragment on App {
    clientId
    urlToken
    httpPort
    httpsPort
    appDir
    deviceName
    deviceType
    capabilities
    buildChannel
    permissions
    downloadsDir
    developerMode
    debug
  }
`,f=`
  fragment ChatItemFragment on ChatItem {
    id
    fromId
    toId
    channelId
    createdAt
    content
    status
    statusData
  }
`,p=`
  fragment SmsFragment on Sms {
    id
    body
    address
    serviceCenter
    sentAt
    type
    threadId
    subscriptionId
    isMms
    attachments {
      path
      contentType
      name
    }
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,m=`
  fragment SmsConversationFragment on SmsConversation {
    id
    address
    snippet
    lastMessageAt
    messageCount
    read
  }
`,h=`
  fragment SmsConversationWithAddressesFragment on SmsConversation {
    id
    address
    addresses
    snippet
    lastMessageAt
    messageCount
    read
  }
`,g=`
  fragment ContactFragment on Contact {
    id
    suffix
    prefix
    firstName
    middleName
    lastName
    updatedAt
    notes
    source
    thumbnailId
    starred
    phoneNumbers {
      value
      type
      label
      normalizedNumber
    }
    addresses {
      value
      type
      label
    }
    emails {
      value
      type
      label
    }
    websites {
      value
      type
      label
    }
    events {
      value
      type
      label
    }
    ims {
      value
      protocol
      customProtocol
    }
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,_=`
  fragment CallFragment on Call {
    id
    name
    number
    durationSec
    accountId
    startedAt
    photoId
    type
    geo {
      country
      numberType
      carrier
      description
    }
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,v=`
  fragment FileFragment on File {
    path
    isDir
    createdAt
    updatedAt
    size
    childCount
    mediaId
  }
`,y=`
  fragment ImageFragment on Image {
    id
    title
    path
    size
    bucketId
    takenAt
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,b=`
  fragment VideoFragment on Video {
    id
    title
    path
    durationMs
    size
    bucketId
    createdAt
    updatedAt
    takenAt
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,ee=`
  fragment AudioFragment on Audio {
    id
    title
    artist
    path
    durationMs
    size
    bucketId
    albumFileId
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,x=`
  fragment NoteFragment on Note {
    id
    title
    content
    deletedAt
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,S=`
  fragment DocFragment on Doc {
    id
    title
    path
    extension
    size
    bucketId
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,C=`
  fragment FeedFragment on Feed {
    id
    name
    url
    logo
    fetchContent
    createdAt
    updatedAt
  }
`,w=`
  fragment FeedEntryFragment on FeedEntry {
    id
    title
    url
    image
    author
    description
    content
    feedId
    rawId
    publishedAt
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${l}
`,T=`
  fragment PackageFragment on Package {
    id
    name
    type
    version
    path
    size
    certs {
      issuer
      subject
      serialNumber
      validFrom
      validTo
    }
    installedAt
    updatedAt
  }
`,te=`
  fragment NotificationFragment on Notification {
    id
    onlyOnce
    isClearable
    appId
    appName
    postedAt
    silent
    title
    body
    actions
    replyActions
  }
`,E=`
  fragment ClipboardItemFragment on ClipboardItem {
    id
    text
    source
    label
    sensitive
    createdAt
  }
`,D=`
  fragment DeviceInfoFragment on DeviceInfo {
    name
    platform
    manufacturer
    model
    osName
    osVersion
    kernelVersion
    appVersion
    appBuildNumber
    language
    cpuArch
    cpuModel
    totalMemory
    totalStorage
    display {
      width
      height
      density
    }
    android {
      sdkVersion
      versionCodeName
      securityPatch
      bootloader
      fingerprint
      hardware
      radioVersion
      board
      buildBrand
      buildNumber
      device
      javaVmVersion
      glEsVersion
      buildTime
    }
  }
`,O=`
  fragment DeviceStatusFragment on DeviceStatus {
    uptimeSec
    batteryLevel
    charging
    temperatures {
      label
      celsius
    }
    cpuUsage
    memoryAvailable
    storageAvailable
  }
`,k=`
  fragment BookmarkFragment on Bookmark {
    id
    url
    title
    faviconPath
    groupId
    pinned
    clickCount
    lastClickedAt
    sortOrder
    createdAt
    updatedAt
  }
`,A=`
  fragment BookmarkGroupFragment on BookmarkGroup {
    id
    name
    collapsed
    sortOrder
    itemCount
    createdAt
    updatedAt
  }
`,j=`
  fragment ChatChannelFragment on ChatChannel {
    actions { isOwner canRename canManageMembers canLeave canDelete }
    id
    name
    ownerId
    members {
      ...ChatChannelMemberFragment
    }
    version
    status
    createdAt
    updatedAt
  }
  
  fragment ChatChannelMemberFragment on ChatChannelMember {
    peerId
    status
  }

`;function M(e){return e instanceof s?e.status===403?`desktop_access_disabled`:e.message:`network_error`}function N(e){if(e)return typeof e==`function`?e():e}function P(c){let l=a(!1),u=a();async function d(e){if(!(c.enabled&&!c.enabled())){l.value=!0;try{let t=e??N(c.variables),n=await o(typeof c.document==`function`?c.document():c.document,t);n.errors?.length?c.handle(n.data,n.errors[0].message):(u.value=n.data,c.handle(n.data,``))}catch(e){if(e instanceof s&&e.cancelled)return;c.handle(void 0,M(e))}finally{l.value=!1}}}d();let f=!0;return t()&&(r(()=>{f=!1}),n(()=>{f=!0})),typeof c.variables==`function`&&e(c.variables,async()=>{await i(),f&&d()},{deep:!0}),typeof c.document==`function`&&e(c.document,async()=>{await i(),f&&d()}),c.enabled&&e(c.enabled,async(e,t)=>{await i(),f&&e&&!t&&d()}),{loading:l,result:u,refetch:d}}function F(e){let t=a(!1),n=a(),r=0,i=0,c=0,l;async function u(a,u={}){let d=++r,f=u.latest?++i:void 0,p=a??N(e.variables),m=p?{...p}:void 0,h={variables:m,requestId:d,meta:u.meta};u.latest?l=d:c++,t.value=!0;try{let t=typeof e.document==`function`?e.document():e.document,r=u.force?await o(t,m,{fresh:!0}):await o(t,m);if(u.latest&&f!==i)return;r.errors?.length?e.handle(r.data,r.errors[0].message,h):(n.value=r.data,e.handle(r.data,``,h))}catch(t){if(t instanceof s&&t.cancelled)return;(!u.latest||f===i)&&e.handle(void 0,M(t),h)}finally{u.latest?l===d&&(l=void 0):c--,t.value=c>0||l!==void 0}}return{loading:t,result:n,fetch:u}}var I=`
  query ($target: String!) {
    chatItems(target: $target, query: "", offset: 0, limit: 200) {
      ...ChatItemFragment
    }
  }
  ${f}
`;`${f}`;var L=`
  query {
    peers {
      id
      name
      ip
      status
      online
      port
      deviceType
      createdAt
      updatedAt
    }
  }
`,R=`
  query {
    chatConversations {
      id
      chat { ...ChatItemFragment }
    }
  }
  ${f}
`,z=`
  query appFiles($offset: Int!, $limit: Int!) {
    appFiles(query: "", offset: $offset, limit: $limit) {
      id
      size
      mimeType
      fileName
      createdAt
      updatedAt
    }
    appFileCount(query: "")
  }
`,B=`
  query ($type: DataType!, $keys: [String!]!) {
    tagRelations(type: $type, keys: $keys) {
      tagId
      key
    }
  }
`,V=`
  query {
    chatChannels {
      ...ChatChannelFragment
    }
  }
  ${j}
`,H=`
  query ($path: String!, $fileName: String) {
    fileInfo(path: $path, fileName: $fileName) {
      ... on FileInfo {
        path
        updatedAt
        size
      }
      data {
        ... on ImageFileInfo {
          width
          height
          location {
            latitude
            longitude
          }
        }
        ... on VideoFileInfo {
          durationMs
          width
          height
          location {
            latitude
            longitude
          }
        }
        ... on AudioFileInfo {
          durationMs
          location {
            latitude
            longitude
          }
        }
      }
    }
  }
`,U=`
  query sms($offset: Int!, $limit: Int!, $query: String!) {
    sms(query: $query, offset: $offset, limit: $limit) {
      ...SmsFragment
    }
    smsCount(query: $query)
  }
  ${p}
`,W=`
  query {
    sims {
      id
      label
      number
      subscriptionId
    }
  }
`,G=`
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(query: $query, offset: $offset, limit: $limit) {
      ...SmsConversationFragment
    }
    smsConversationCount(query: $query)
  }
  ${m}
`,K=`
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(query: $query, offset: $offset, limit: $limit) {
      ...SmsConversationWithAddressesFragment
    }
    smsConversationCount(query: $query)
  }
  ${h}
`,q=`
  query contacts($offset: Int!, $limit: Int!, $query: String!) {
    contacts(query: $query, offset: $offset, limit: $limit) {
      ...ContactFragment
    }
    contactCount(query: $query)
  }
  ${g}
`,J=`
  query {
    contactSources {
      name
      type
    }
  }
`,Y=`
  query calls($offset: Int!, $limit: Int!, $query: String!) {
    calls(query: $query, offset: $offset, limit: $limit) {
      ...CallFragment
    }
    callCount(query: $query)
  }
  ${_}
`,X=`
  query images($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    images(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...ImageFragment
    }
    imageCount(query: $query)
  }
  ${y}
`,Z=`
  query {
    imageSearchStatus {
      status
      downloadProgress
      errorMessage
      modelSize
      modelDir
      isIndexing
      totalImages
      indexedImages
    }
  }
`,Q=`
  query {
    scanProgress {
      indexed
      pending
      total
      state
      error
    }
  }
`,ne=`
  query videos($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    videos(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...VideoFragment
    }
    videoCount(query: $query)
  }
  ${b}
`,re=`
  query audios($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: audios(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...AudioFragment
    }
    total: audioCount(query: $query)
  }
  ${ee}
`,ie=`
  query files($root: String!, $offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    files(root: $root, query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...FileFragment
    }
  }
  ${v}
`,ae=`
  query recentFiles {
    recentFiles {
      ...FileFragment
    }
  }
  ${v}
`,oe=`
  query {
    mounts {
      id
      name
      path
      mountPoint
      fsType
      totalBytes
      usedBytes
      freeBytes
      remote
      alias
      driveType
      diskId
    }
  }
`,se=`
  query {
    disks {
      id
      name
      path
      sizeBytes
      removable
      model
    }
  }
`,ce=`
  query {
    sambaSettings {
      enabled
      username
      hasPassword
      shares {
        name
        sharePath
        auth
        readOnly
      }
      serviceName
      serviceActive
      serviceEnabled
    }
  }
`,le=`
  query {
    favoriteFolders {
      rootPath
      fullPath
      alias
    }
  }
`,ue=`
  query {
    app {
      ...AppFragment
    }
  }
  ${d}
`,de=`
  query audioQueue($offset: Int!, $limit: Int!) {
    items: audioQueueItems(query: "", offset: $offset, limit: $limit) {
      ...AudioItemFragment
    }
    total: audioQueueItemCount
    playback: audioPlayback {
      revision
      mode
      mediaId
      path
      isPlaying
      positionMs
    }
  }
  ${u}
`,fe=`
  query tags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
  }
  ${c}
`,pe=`
  query mediaBuckets($type: MediaDataType!) {
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItemPaths
    }
  }
`,me=`
  query notes($offset: Int!, $limit: Int!, $query: String!) {
    notes(query: $query, offset: $offset, limit: $limit) {
      id
      title
      deletedAt
      createdAt
      updatedAt
      tags {
        ...TagSubFragment
      }
    }
    noteCount(query: $query)
  }
  ${l}
`,he=`
  query note($id: ID!) {
    note(id: $id) {
      ...NoteFragment
    }
  }
  ${x}
`,ge=`
  query {
    feeds {
      ...FeedFragment
    }
  }
  ${C}
`,_e=`
  query feedEntries($offset: Int!, $limit: Int!, $query: String!) {
    items: feedEntries(query: $query, offset: $offset, limit: $limit) {
      id
      title
      url
      image
      author
      feedId
      rawId
      publishedAt
      createdAt
      updatedAt
      tags {
        ...TagSubFragment
      }
    }
    total: feedEntryCount(query: $query)
  }
  ${l}
`,ve=`
  query feedsTags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
    feeds {
      ...FeedFragment
    }
  }
  ${C}
  ${c}
`,ye=`
  query bucketsTags($type: MediaDataType!, $tagType: DataType!) {
    tags(type: $tagType) {
      ...TagFragment
    }
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItemPaths
    }
  }
  ${c}
`,be=`
  query feedEntry($id: ID!) {
    feedEntry(id: $id) {
      ...FeedEntryFragment
      feed {
        ...FeedFragment
      }
    }
  }
  ${C}
  ${w}
`,xe=`
  query imageCount($query: String!) {
    total: imageCount(query: $query)
    trash: imageCount(query: "trash:true")
  }
`,Se=`
  query audioCount($query: String!) {
    total: audioCount(query: $query)
    trash: audioCount(query: "trash:true")
  }
`,Ce=`
  query docs($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: docs(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...DocFragment
    }
    total: docCount(query: $query)
  }
  ${S}
`,we=`
  query docCount($query: String!) {
    total: docCount(query: $query)
    trash: docCount(query: "trash:true")
    extGroups: docExtGroups {
      ext
      count
    }
  }
`,Te=`
  query videoCount($query: String!) {
    total: videoCount(query: $query)
    trash: videoCount(query: "trash:true")
  }
`,Ee=`
  query {
    total: packageCount(query: "")
    system: packageCount(query: "type:SYSTEM")
  }
`,De=`
  query {
    total: feedEntryCount(query: "")
    today: feedEntryCount(query: "today:true")
    feedEntryCounts {
      id
      count
    }
  }
`,Oe=`
  query {
    total: contactCount(query: "")
  }
`,ke=`
  query {
    total: callCount(query: "")
    incoming: callCount(query: "type:1")
    outgoing: callCount(query: "type:2")
    missed: callCount(query: "type:3")
  }
`,Ae=`
  query {
    smsBoxCounts {
      total
      inbox
      sent
      drafts
    }
  }
`,je=`
  query {
    archivedSmsConversations(query: "", offset: 0, limit: 200) {
      ...SmsConversationFragment
    }
  }
  ${m}
`,Me=`
  query {
    archivedSmsConversations(query: "", offset: 0, limit: 200) {
      ...SmsConversationWithAddressesFragment
    }
  }
  ${h}
`,Ne=`
  query {
    total: noteCount(query: "")
    trash: noteCount(query: "trash:true")
  }
`,Pe=`
  query packages($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    packages(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...PackageFragment
    }
    packageCount(query: $query)
  }
  ${T}
`,Fe=`
  query packageStatuses($ids: [ID!]!) {
    packageStatuses(ids: $ids) {
      id
      exists
      updatedAt
    }
  }
`,Ie=`
  query {
    isScreenMirroring
    screenMirrorControlEnabled
    screenMirrorQuality {
      mode
      resolution
    }
  }
`,Le=`
  query {
    screenMirrorVideoCodec {
      annexB
      keyFrame
    }
  }
`,Re=`
  query {
    screenMirrorControlEnabled
  }
`,ze=`
  mutation {
    requestScreenMirrorKeyFrame
  }
`,Be=`
  query {
    notifications(query: "", offset: 0, limit: 200) {
      ...NotificationFragment
    }
  }
  ${te}
`,Ve=`
  query clipboardItems($offset: Int!, $limit: Int!, $query: String!) {
    clipboardItems(query: $query, offset: $offset, limit: $limit) {
      ...ClipboardItemFragment
    }
    clipboardItemCount(query: $query)
  }
  ${E}
`,He=`
  query {
    deviceInfo {
      ...DeviceInfoFragment
    }
    deviceStatus {
      ...DeviceStatusFragment
    }
  }
  ${D}
  ${O}
`,Ue=`
  query {
    deviceStatus {
      batteryLevel
      charging
    }
  }
`,We=`
  query AppLogs($offset: Int!, $limit: Int!) {
    appLogs(query: "", offset: $offset, limit: $limit)
  }
`,Ge=`
  query {
    appLogPath
  }
`,Ke=`
  query {
    dbPath
  }
`,qe=`
  query uploadedChunks($fileId: String!) {
    uploadedChunks(fileId: $fileId)
  }
`,Je=`
  query mergeStatus($fileId: String!) {
    mergeStatus(fileId: $fileId) {
      status
      value
      mergedSize
      error
    }
  }
`,$=`
  query {
    pomodoroToday {
      date
      completedCount
      currentRound
      timeLeftSec
      totalTimeSec
      isRunning
      isPaused
      state
    }
    pomodoroSettings {
      workDurationMin
      shortBreakDurationMin
      longBreakDurationMin
      pomodorosBeforeLongBreak
      showNotification
      playSoundOnComplete
    }
  }
`,Ye=`
  query {
    prefs
  }
`,Xe=`
  query {
    dbTables
  }
`,Ze=`
  query DbTableRowCount($table: String!) {
    dbTableRowCount(table: $table)
  }
`,Qe=`
  query DbTableRows($table: String!, $offset: Int!, $limit: Int!) {
    dbTableRows(table: $table, offset: $offset, limit: $limit)
  }
`,$e=`
  query DbTableInfo($table: String!) {
    dbTableInfo(table: $table) {
      idKey
    }
  }
`,et=`
  query DbTableColumns($table: String!) {
    dbTableColumns(table: $table) {
      name
      dataType
      notNull
      defaultValue
      primaryKey
    }
  }
`,tt=`
  query {
    bookmarks {
      ...BookmarkFragment
    }
    bookmarkGroups {
      ...BookmarkGroupFragment
    }
  }
  ${k}
  ${A}
`,nt=`
  query appUpdate {
    appUpdate { currentVersion latestVersion hasUpdate url releaseName publishedAt }
  }
`,rt=`
  query NearbyDevices {
    nearbyDevices { id name ips bestIp port deviceType version platform lastSeen status discoveryMethods }
    pairingStates { deviceId generation mode phase error }
  }
`;export{me as $,se as A,w as At,ie as B,et as C,u as Ct,Xe as D,f as Dt,Qe as E,j as Et,De as F,P as G,Z as H,be as I,Je as J,Ie as K,ge as L,Ce as M,v as Mt,le as N,x as Nt,He as O,E as Ot,_e as P,c as Pt,he as Q,ve as R,Ke as S,ne as St,Ze as T,A as Tt,X as U,xe as V,F as W,rt as X,oe as Y,Ne as Z,I as _,U as _t,nt as a,$ as at,J as b,qe as bt,Se as c,ze as ct,tt as d,Re as dt,Be as et,ye as f,Le as ft,R as g,Ae as gt,V as h,K as ht,We as i,L as it,we as j,C as jt,Ue as k,g as kt,de as l,ce as lt,Y as m,G as mt,ue as n,Fe as nt,je as o,Ye as ot,ke as p,W as pt,pe as q,Ge as r,Pe as rt,Me as s,ae as st,z as t,Ee as tt,re as u,Q as ut,Ve as v,B as vt,$e as w,k as wt,q as x,Te as xt,Oe as y,fe as yt,H as z};