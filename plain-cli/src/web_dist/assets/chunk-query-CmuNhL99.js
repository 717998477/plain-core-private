import{a as e}from"./chunk-core-Cig7zpmj.js";import{Ht as t,L as n,ft as r,nt as i,ot as a,tt as o,vn as s}from"./chunk-runtime-core.esm-bundler-DrnlzXx3.js";import{r as c}from"./chunk-prefs-yQ8vbfqa.js";import{A as l,G as u,K as d,O as f,U as p,bt as m,k as h,nt as g,p as ee,vt as _}from"./chunk-file-Dm0qfhns.js";import{a as v,c as y,d as b,t as x}from"./chunk-gql-client-c7jKyGuC.js";var S=new TextEncoder;function C(e){let t=new Uint8Array(e.reduce((e,t)=>e+4+t.length,0)),n=0;for(let r of e)new DataView(t.buffer).setUint32(n,r.length,!1),t.set(r,n+4),n+=4+r.length;return t}function w(e,t,n){let r=f(24),i=h(e,r,n).encrypt(t),a=new Uint8Array(24+i.length);return a.set(r),a.set(i,24),a}function T(e,t,n){return h(e,t.subarray(0,24),n).decrypt(t.subarray(24))}var te=class{client;key;registration;binding;sent=0n;received=0n;constructor(e,t){this.client=e,this.key=t;let n=[...f(32)].map(e=>e.toString(16).padStart(2,`0`)).join(``),r=w(t,S.encode(JSON.stringify({timestampMillis:y(),clientNonce:n})),C([S.encode(`plain-graphql-init-v1`),S.encode(`/graphql`),S.encode(e)]));this.registration=new Uint8Array(1+r.length),this.registration[0]=1,this.registration.set(r,1)}acknowledge(e){if(e[0]!==1||this.binding)throw Error(`Invalid subscription acknowledgement`);let t=C([S.encode(`plain-graphql-ack-v1`),S.encode(`/graphql`),S.encode(this.client),l(this.registration)]),n=JSON.parse(new TextDecoder().decode(T(this.key,e.subarray(1),t)));if(!/^[0-9a-f]{64}$/.test(n.serverNonce))throw Error(`Invalid subscription server nonce`);this.binding=l(C([this.registration,e]))}aad(e,t){if(!this.binding)throw Error(`Subscription handshake incomplete`);let n=new Uint8Array(8);return new DataView(n.buffer).setBigUint64(0,t,!1),C([S.encode(`plain-graphql-frame-v1`),S.encode(`/graphql`),S.encode(this.client),this.binding,S.encode(e),n])}encode(e){if(this.sent===18446744073709551615n)throw Error(`Subscription sequence exhausted`);let t=w(this.key,S.encode(JSON.stringify(e)),this.aad(`client`,this.sent)),n=new Uint8Array(9+t.length);return n[0]=1,new DataView(n.buffer).setBigUint64(1,this.sent,!1),n.set(t,9),this.sent++,n}decode(e){if(e[0]!==1||e.length<49)throw Error(`Invalid subscription frame`);let t=new DataView(e.buffer,e.byteOffset,e.byteLength).getBigUint64(1,!1);if(t!==this.received||t===18446744073709551615n)throw Error(`Invalid subscription sequence`);let n=T(this.key,e.subarray(9),this.aad(`server`,t));return this.received++,JSON.parse(new TextDecoder().decode(n))}};function ne(e){return d()?ie(e):re(e)}function re(e){let t=new URL(`/graphql`,u()),n=c(`client_id`,``),r=m()?p():g();if(!r)throw Error(`Subscription session missing`);t.searchParams.set(`cid`,n);let i=b(t.toString(),m()?``:_(),`plain-graphql-ws-v1`);i.binaryType=`arraybuffer`;let a=!0,o=!1,s,l,d,f=setTimeout(()=>v(Error(`Subscription handshake timeout`)),1e4),h={send(e){try{if(!a||!o||!s)throw Error(`Subscription connection unavailable`);i.send(s.encode(e))}catch(e){v(e)}},close(){a=!1,s=void 0,clearTimeout(f),clearTimeout(d),clearInterval(l),i.close()}};function v(t){a&&(h.close(),e.dropped(t))}return i.onopen=()=>{if(a){if(i.protocol!==`plain-graphql-ws-v1`){v(Error(`Subscription protocol mismatch`));return}try{s=new te(n,ee(r)),i.send(s.registration)}catch(e){v(e)}}},i.onmessage=t=>{if(a)try{if(!s||!(t.data instanceof ArrayBuffer))throw Error(`Encrypted subscription frame required`);if(!o)s.acknowledge(new Uint8Array(t.data)),o=!0,clearTimeout(f),e.opened(),l=setInterval(()=>{h.send({type:`ping`}),d=setTimeout(()=>v(Error(`Subscription heartbeat timeout`)),1e4)},2e4);else{let n=s.decode(new Uint8Array(t.data));typeof n==`object`&&n&&`type`in n&&n.type===`pong`?clearTimeout(d):e.message(n)}}catch(e){v(e)}},i.onerror=()=>v(Error(`Subscription connection error`)),i.onclose=()=>v(Error(`Subscription connection closed`)),h}function ie(t){let n=!0,r,i=Promise.resolve(),a={send(t){if(!n||r===void 0)throw Error(`Native subscription unavailable`);let a=r;i=i.then(()=>n?e(`native_subscription_send`,{handle:a,message:JSON.stringify(t)}):void 0).catch(o)},close(){n=!1,r!==void 0&&e(`native_subscription_close`,{handle:r}).catch(console.error),r=void 0}};function o(e){n&&(a.close(),t.dropped(e))}return(async()=>{let i=await e(`native_subscription_open`);if(!n){await e(`native_subscription_close`,{handle:i});return}for(r=i,t.opened();n;){let r=await e(`native_subscription_read`,{handle:i});n&&t.message(JSON.parse(r))}})().catch(o),a}var E=new class{connection;operations=new Map;ids=new Map;sequence=0;pending=new Set;acknowledged=!1;active=!1;retry;failures=0;generation=0;consumers=new Map;available=[];attachments=new Set;connectionListeners=new Set;onConnectionChange(e){return this.connectionListeners.add(e),()=>{this.connectionListeners.delete(e)}}subscribe(e,t){let n=this.consumers.get(e)??new Set;this.consumers.set(e,n);let r=e=>t(e);return n.add(r),this.updateOperations(),()=>{n.delete(r)&&(n.size||this.consumers.delete(e),this.updateOperations())}}onAttached(e){let t=()=>e();return this.attachments.add(t),()=>{this.attachments.delete(t)}}ready(){for(let e of[...this.attachments])if(this.attachments.has(e))try{e()}catch(e){console.error(`GraphQL subscription snapshot failed`,e)}}deliver(e,t){let n=this.consumers.get(e.field);if(n){for(let r of[...n])if(n.has(r))try{let n=r(t);n&&n.catch(t=>console.error(`GraphQL subscription consumer failed`,e.field,t))}catch(t){console.error(`GraphQL subscription consumer failed`,e.field,t)}}}start(){this.active=!0,this.connect()}setOperations(e){this.available=e,this.updateOperations()}updateOperations(){let e=new Map(this.available.filter(e=>this.consumers.has(e.field)).map(e=>[e.field,e]));for(let t of this.operations.keys())if(!e.has(t))for(let[e,n]of this.ids)n===t&&(this.acknowledged&&this.connection?.send({id:e,type:`complete`}),this.ids.delete(e),this.pending.delete(e));for(let[t,n]of e)!this.operations.has(t)&&this.acknowledged&&this.send(n);this.operations=e}send(e){let t=`${this.generation}:${++this.sequence}`;this.ids.set(t,e.field),this.pending.add(t),this.connection?.send({type:`subscribe`,id:t,payload:{query:e.document}})}connect(){if(!this.active)return;let e=++this.generation;try{this.connection=ne({opened:()=>{if(e===this.generation){this.acknowledged=!0;for(let e of this.connectionListeners)e(!0);this.failures=0,this.pending.clear(),this.ids.clear();for(let e of this.operations.values())this.send(e)}},message:t=>{e===this.generation&&this.receive(t)},dropped:t=>{e===this.generation&&this.dropped(t)}})}catch(e){this.dropped(e)}}receive(e){if(!e||typeof e!=`object`||!(`type`in e)||!(`id`in e)||typeof e.id!=`string`)throw Error(`Invalid subscription message`);let t=e,n=this.operations.get(this.ids.get(t.id)??``);if(n)if(t.type===`next`){if(!t.payload||Array.isArray(t.payload))throw Error(`Invalid subscription result`);t.payload.extensions?.subscriptionReady&&this.settled(t.id),t.payload.errors?.length&&console.error(`GraphQL subscription errors`,n.field,t.payload.errors),t.payload.data&&Object.hasOwn(t.payload.data,n.field)&&this.deliver(n,t.payload.data[n.field])}else if(t.type===`error`){if(!Array.isArray(t.payload))throw Error(`Invalid subscription error`);if(console.error(`GraphQL subscription failed`,n.field,t.payload),t.payload.some(e=>e.message===`resync_required`)){this.dropped(Error(`Subscription resynchronization required`),!0);return}this.ids.delete(t.id),this.settled(t.id)}else if(t.type===`complete`)this.ids.delete(t.id),this.settled(t.id);else throw Error(`Unknown subscription message`)}settled(e){this.pending.delete(e)&&this.pending.size===0&&this.ready()}dropped(e,t=!1){console.error(e),this.generation++,this.connection?.close(),this.acknowledged=!1;for(let e of this.connectionListeners)e(!1);this.connection=void 0,this.pending.clear(),this.ids.clear(),this.active&&(t||!d())&&(this.retry=setTimeout(()=>this.connect(),Math.min(5e3,++this.failures*1e3)))}stop(){this.active=!1;for(let e of this.connectionListeners)e(null);this.generation++,clearTimeout(this.retry),this.acknowledged=!1,this.connection?.close(),this.connection=void 0,this.pending.clear(),this.ids.clear()}},D=`
  fragment TagFragment on Tag {
    id
    name
    count
  }
`,O=`
  fragment TagSubFragment on Tag {
    id
    name
  }
`,k=`
  fragment AudioItemFragment on AudioItem {
    mediaId
    title
    artist
    path
    durationMs
  }
`,A=`
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
`,j=`
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
`,ae=`
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
  ${O}
`,M=`
  fragment SmsConversationFragment on SmsConversation {
    id
    address
    snippet
    lastMessageAt
    messageCount
    read
  }
`,N=`
  fragment SmsConversationWithAddressesFragment on SmsConversation {
    id
    address
    addresses
    snippet
    lastMessageAt
    messageCount
    read
  }
`,P=`
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
  ${O}
`,oe=`
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
  ${O}
`,F=`
  fragment FileFragment on File {
    path
    isDir
    createdAt
    updatedAt
    size
    childCount
    mediaId
  }
`,I=`
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
  ${O}
`,L=`
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
  ${O}
`,R=`
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
  ${O}
`,z=`
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
  ${O}
`,B=`
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
  ${O}
`,V=`
  fragment FeedFragment on Feed {
    id
    name
    url
    logo
    fetchContent
    createdAt
    updatedAt
  }
`,H=`
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
  ${O}
`,U=`
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
`,W=`
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
`,G=`
  fragment ClipboardItemFragment on ClipboardItem {
    id
    text
    source
    label
    sensitive
    createdAt
  }
`,K=`
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
`,q=`
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
`,J=`
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
`,Y=`
  fragment BookmarkGroupFragment on BookmarkGroup {
    id
    name
    collapsed
    sortOrder
    itemCount
    createdAt
    updatedAt
  }
`,X=`
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

`;function Z(e){return e instanceof x?e.status===403?`desktop_access_disabled`:e.message:`network_error`}function Q(e){if(e)return typeof e==`function`?e():e}function se(e){let c=s(!1),l=s(),u=0;async function d(t,n=!1){if(e.enabled&&!e.enabled())return;let r=u;c.value=!0;try{let i=t??Q(e.variables),a=await v(typeof e.document==`function`?e.document():e.document,i,{fresh:n});if(r!==u)return;a.errors?.length?e.handle(a.data,a.errors[0].message):(l.value=a.data,e.handle(a.data,``))}catch(t){if(r!==u||t instanceof x&&t.cancelled)return;e.handle(void 0,Z(t))}finally{r===u&&(c.value=!1)}}d();let f=!0,p=n();return p&&(a(()=>{f=!1}),i(()=>{f=!0})),typeof e.variables==`function`&&t(e.variables,async()=>{await o(),f&&d()},{deep:!0}),typeof e.document==`function`&&t(e.document,async()=>{await o(),f&&d()}),e.enabled&&t(e.enabled,async(e,t)=>{await o(),f&&e&&!t&&d()}),p&&r(E.onAttached(()=>{f&&(u++,d(void 0,!0))})),{loading:c,result:l,refetch:d}}function ce(e){let t=s(!1),o=s(),c=0,l=0,u=0,d,f=0,p=!0,m;n()&&(i(()=>{p=!0}),a(()=>{p=!1}),r(E.onAttached(()=>{p&&m&&(f++,h(m.variables,{...m.options,latest:!0,force:!0}))})));async function h(n,r={}){let i=f;m={variables:n,options:r};let a=++c,s=r.latest?++l:void 0,p=n??Q(e.variables),h=p?{...p}:void 0,g={variables:h,requestId:a,meta:r.meta};r.latest?d=a:u++,t.value=!0;try{let t=typeof e.document==`function`?e.document():e.document,n=r.force?await v(t,h,{fresh:!0}):await v(t,h);if(i!==f||r.latest&&s!==l)return;n.errors?.length?e.handle(n.data,n.errors[0].message,g):(o.value=n.data,e.handle(n.data,``,g))}catch(t){if(i!==f||t instanceof x&&t.cancelled)return;(!r.latest||s===l)&&e.handle(void 0,Z(t),g)}finally{r.latest?d===a&&(d=void 0):u--,t.value=u>0||d!==void 0}}return{loading:t,result:o,fetch:h}}var le=`
  query ($target: String!) {
    chatItems(target: $target, query: "", offset: 0, limit: 200) {
      ...ChatItemFragment
    }
  }
  ${j}
`;`${j}`;var ue=`
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
`,de=`
  query {
    chatConversations {
      id
      chat { ...ChatItemFragment }
    }
  }
  ${j}
`,fe=`
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
`,pe=`
  query ($type: DataType!, $keys: [String!]!) {
    tagRelations(type: $type, keys: $keys) {
      tagId
      key
    }
  }
`,me=`
  query {
    chatChannels {
      ...ChatChannelFragment
    }
  }
${X}
`,he=`
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
`,ge=`
  query sms($offset: Int!, $limit: Int!, $query: String!) {
    sms(query: $query, offset: $offset, limit: $limit) {
      ...SmsFragment
    }
    smsCount(query: $query)
  }
  ${ae}
`,_e=`
  query {
    sims {
      id
      label
      number
      subscriptionId
    }
  }
`,ve=`
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(query: $query, offset: $offset, limit: $limit) {
      ...SmsConversationFragment
    }
    smsConversationCount(query: $query)
  }
  ${M}
`,ye=`
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(query: $query, offset: $offset, limit: $limit) {
      ...SmsConversationWithAddressesFragment
    }
    smsConversationCount(query: $query)
  }
  ${N}
`,be=`
  query contacts($offset: Int!, $limit: Int!, $query: String!) {
    contacts(query: $query, offset: $offset, limit: $limit) {
      ...ContactFragment
    }
    contactCount(query: $query)
  }
  ${P}
`,xe=`
  query {
    contactSources {
      name
      type
    }
  }
`,Se=`
  query calls($offset: Int!, $limit: Int!, $query: String!) {
    calls(query: $query, offset: $offset, limit: $limit) {
      ...CallFragment
    }
    callCount(query: $query)
  }
  ${oe}
`,Ce=`
  query images($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    images(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...ImageFragment
    }
    imageCount(query: $query)
  }
  ${I}
`,we=`
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
`,Te=`
  query {
    scanProgress {
      indexed
      pending
      total
      state
      error
    }
  }
`,Ee=`
  query videos($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    videos(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...VideoFragment
    }
    videoCount(query: $query)
  }
  ${L}
`,De=`
  query audios($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: audios(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...AudioFragment
    }
    total: audioCount(query: $query)
  }
  ${R}
`,Oe=`
  query files($root: String!, $offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    files(root: $root, query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...FileFragment
    }
  }
  ${F}
`,ke=`
  query recentFiles {
    recentFiles {
      ...FileFragment
    }
  }
  ${F}
`,Ae=`
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
`,je=`
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
`,Me=`
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
`,Ne=`
  query {
    favoriteFolders {
      rootPath
      fullPath
      alias
    }
  }
`,Pe=`
  query {
    app {
      ...AppFragment
    }
  }
  ${A}
`,Fe=`
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
  ${k}
`,Ie=`
  query tags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
  }
  ${D}
`,Le=`
  query mediaBuckets($type: MediaDataType!) {
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItemPaths
    }
  }
`,Re=`
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
  ${O}
`,ze=`
  query note($id: ID!) {
    note(id: $id) {
      ...NoteFragment
    }
  }
  ${z}
`,Be=`
  query {
    feeds {
      ...FeedFragment
    }
  }
  ${V}
`,Ve=`
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
  ${O}
`,He=`
  query feedsTags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
    feeds {
      ...FeedFragment
    }
  }
  ${V}
  ${D}
`,Ue=`
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
  ${D}
`,We=`
  query feedEntry($id: ID!) {
    feedEntry(id: $id) {
      ...FeedEntryFragment
      feed {
        ...FeedFragment
      }
    }
  }
  ${V}
  ${H}
`,Ge=`
  query imageCount($query: String!) {
    total: imageCount(query: $query)
    trash: imageCount(query: "trash:true")
  }
`,Ke=`
  query audioCount($query: String!) {
    total: audioCount(query: $query)
    trash: audioCount(query: "trash:true")
  }
`,qe=`
  query docs($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: docs(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...DocFragment
    }
    total: docCount(query: $query)
  }
  ${B}
`,Je=`
  query docCount($query: String!) {
    total: docCount(query: $query)
    trash: docCount(query: "trash:true")
    extGroups: docExtGroups {
      ext
      count
    }
  }
`,Ye=`
  query videoCount($query: String!) {
    total: videoCount(query: $query)
    trash: videoCount(query: "trash:true")
  }
`,Xe=`
  query {
    total: packageCount(query: "")
    system: packageCount(query: "type:SYSTEM")
  }
`,Ze=`
  query {
    total: feedEntryCount(query: "")
    today: feedEntryCount(query: "today:true")
    feedEntryCounts {
      id
      count
    }
  }
`,Qe=`
  query {
    total: contactCount(query: "")
  }
`,$e=`
  query {
    total: callCount(query: "")
    incoming: callCount(query: "type:1")
    outgoing: callCount(query: "type:2")
    missed: callCount(query: "type:3")
  }
`,et=`
  query {
    smsBoxCounts {
      total
      inbox
      sent
      drafts
    }
  }
`,tt=`
  query {
    archivedSmsConversations(query: "", offset: 0, limit: 200) {
      ...SmsConversationFragment
    }
  }
  ${M}
`,nt=`
  query {
    archivedSmsConversations(query: "", offset: 0, limit: 200) {
      ...SmsConversationWithAddressesFragment
    }
  }
  ${N}
`,rt=`
  query {
    total: noteCount(query: "")
    trash: noteCount(query: "trash:true")
  }
`,it=`
  query packages($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    packages(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...PackageFragment
    }
    packageCount(query: $query)
  }
  ${U}
`,at=`
  query packageStatuses($ids: [ID!]!) {
    packageStatuses(ids: $ids) {
      id
      exists
      updatedAt
    }
  }
`,ot=`
  query {
    isScreenMirroring
    screenMirrorControlEnabled
    screenMirrorQuality {
      mode
      resolution
    }
  }
`,st=`
  query {
    screenMirrorVideoCodec {
      annexB
      keyFrame
    }
  }
`,ct=`
  query {
    screenMirrorControlEnabled
  }
`,lt=`
  mutation {
    requestScreenMirrorKeyFrame
  }
`,ut=`
  query {
    notifications(query: "", offset: 0, limit: 200) {
      ...NotificationFragment
    }
  }
  ${W}
`,dt=`
  query clipboardItems($offset: Int!, $limit: Int!, $query: String!) {
    clipboardItems(query: $query, offset: $offset, limit: $limit) {
      ...ClipboardItemFragment
    }
    clipboardItemCount(query: $query)
  }
  ${G}
`,ft=`
  query {
    deviceInfo {
      ...DeviceInfoFragment
    }
    deviceStatus {
      ...DeviceStatusFragment
    }
  }
  ${K}
  ${q}
`,pt=`
  query {
    deviceStatus {
      batteryLevel
      charging
    }
  }
`,mt=`
  query AppLogs($offset: Int!, $limit: Int!) {
    appLogs(query: "", offset: $offset, limit: $limit)
  }
`,ht=`
  query {
    appLogPath
  }
`,gt=`
  query {
    dbPath
  }
`,_t=`
  query uploadedChunks($fileId: String!) {
    uploadedChunks(fileId: $fileId)
  }
`,vt=`
  query mergeStatus($fileId: String!) {
    mergeStatus(fileId: $fileId) {
      status
      value
      mergedSize
      error
    }
  }
`,yt=`
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
`,$=`
  query {
    prefs
  }
`,bt=`
  query {
    dbTables
  }
`,xt=`
  query DbTableRowCount($table: String!) {
    dbTableRowCount(table: $table)
  }
`,St=`
  query DbTableRows($table: String!, $offset: Int!, $limit: Int!) {
    dbTableRows(table: $table, offset: $offset, limit: $limit)
  }
`,Ct=`
  query DbTableInfo($table: String!) {
    dbTableInfo(table: $table) {
      idKey
    }
  }
`,wt=`
  query DbTableColumns($table: String!) {
    dbTableColumns(table: $table) {
      name
      dataType
      notNull
      defaultValue
      primaryKey
    }
  }
`,Tt=`
  query {
    bookmarks {
      ...BookmarkFragment
    }
    bookmarkGroups {
      ...BookmarkGroupFragment
    }
  }
  ${J}
  ${Y}
`,Et=`
  query appUpdate {
    appUpdate { currentVersion latestVersion hasUpdate url releaseName publishedAt }
  }
`,Dt=`
  query NearbyDevices {
    nearbyDevices { id name ips bestIp port deviceType version platform lastSeen status discoveryMethods }
    pairingStates { deviceId generation mode phase error }
    mdnsState { scanning receiver }
  }
`;export{Re as $,je as A,H as At,Oe as B,wt as C,k as Ct,bt as D,j as Dt,St as E,X as Et,Ze as F,E as Ft,se as G,we as H,We as I,vt as J,ot as K,Be as L,qe as M,F as Mt,Ne as N,z as Nt,ft as O,G as Ot,Ve as P,D as Pt,ze as Q,He as R,gt as S,Ee as St,xt as T,Y as Tt,Ce as U,Ge as V,ce as W,Dt as X,Ae as Y,rt as Z,le as _,ge as _t,Et as a,yt as at,xe as b,_t as bt,Ke as c,lt as ct,Tt as d,ct as dt,ut as et,Ue as f,st as ft,de as g,et as gt,me as h,ye as ht,mt as i,ue as it,Je as j,V as jt,pt as k,P as kt,Fe as l,Me as lt,Se as m,ve as mt,Pe as n,at as nt,tt as o,$ as ot,$e as p,_e as pt,Le as q,ht as r,it as rt,nt as s,ke as st,fe as t,Xe as tt,De as u,Te as ut,dt as v,pe as vt,Ct as w,J as wt,be as x,Ye as xt,Qe as y,Ie as yt,he as z};