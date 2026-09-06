import type { NetworkConnectionStatus, Profile, Tab } from "../model";

interface Props {
  profile: Profile;
  tab: Tab;
  networkStatus: NetworkConnectionStatus;
  reconnectAttempts: number;
  incomingRequests: number;
  onTab: (tab: Tab) => void;
  onSwitchUser: () => void;
}

const tabs: Tab[] = ["Chats", "Contacts", "Discover", "Rooms", "Games", "Kaspa", "Settings"];

export function Sidebar({ profile, tab, networkStatus, reconnectAttempts, incomingRequests, onTab, onSwitchUser }: Props) {
  const networkTitle = networkStatus === "reconnecting"
    ? `Kaspa reconnecting${reconnectAttempts > 0 ? ` · attempt ${reconnectAttempts}` : ""}`
    : `Kaspa ${networkStatus}`;
  return (
    <aside className="sidebar">
      <div className="brand-row">
        <img className="brand-mark small" src="./ghost-talk-icon.png" alt="" />
        <h1>Ghost Talk</h1>
        <span className={`network-dot ${networkStatus}`} title={networkTitle} aria-label={networkTitle} />
      </div>
      <input className="search" placeholder="Search name, KNS or kaspa:…" />
      <nav>
        {tabs.map(name => (
          <button key={name} className={tab === name ? "active" : ""} onClick={() => onTab(name)}>
            <span>{tabIcon(name)}</span><span className="nav-label">{name}</span>
            {name === "Chats" && incomingRequests > 0 && <span className="nav-badge" title={`${incomingRequests} incoming chat request${incomingRequests === 1 ? "" : "s"}`}>{incomingRequests}</span>}
          </button>
        ))}
      </nav>
      <button className="profile-chip" onClick={onSwitchUser}>
        <span className="avatar">{profile.label.slice(0, 1).toUpperCase()}</span>
        <span><b>{profile.label}</b><small>{profile.wallet?.public.network ?? "No Kaspa wallet"}</small></span>
        <span>⇄</span>
      </button>
    </aside>
  );
}

function tabIcon(tab: Tab): string {
  return ({ Chats: "◉", Contacts: "◎", Discover: "✦", Rooms: "▦", Games: "◇", Kaspa: "K", Settings: "⚙" })[tab];
}
