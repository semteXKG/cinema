export function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="box">
        <span className="icon tg">
          <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
            <circle cx="24" cy="24" r="24" fill="#229ED9" />
            <path
              fill="#fff"
              d="M10.7 23.5l25-9.6c1.2-.4 2.2.3 1.8 2l-4.3 20c-.3 1.3-1 1.6-2 1l-6-4.4-2.9 2.8c-.3.3-.6.6-1.2.6l.4-6 10.6-9.6c.5-.4-.1-.6-.7-.2L17.2 22l-5.9-1.8c-1.3-.4-1.3-1.3.3-2z"
            />
          </svg>
        </span>
        <span className="text">
          Get notified about new OV showings on Telegram
          <span className="sub">Channel: @ov_linz — free, no spam, only new showings.</span>
        </span>
        <a href="https://t.me/ov_linz" target="_blank" rel="noopener">
          JOIN
        </a>
      </div>
      <div className="box">
        <span className="icon">📅</span>
        <span className="text">
          Add showings to your calendar
          <span className="sub">Subscribe in Google, Apple or Outlook Calendar.</span>
        </span>
        <a href="/showings.ics">SUBSCRIBE</a>
      </div>
    </aside>
  );
}
