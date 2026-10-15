// A fake Door43 (Gitea API v1) for the share screens, served from the Playwright browser
// context so no request leaves the machine. A trimmed port of tC4's own e2e fake
// (e2e/helpers/door43.ts at d494a9d): sign-in with Basic auth, token mint, the user's
// organizations with their create permission, and the per-language repository count that
// picks the "Recommended" organization. Repository creation and push are not served.

const CORS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, PUT, OPTIONS, PATCH, DELETE',
  'Access-Control-Allow-Headers': 'Authorization,Content-Type,Accept',
  'Access-Control-Expose-Headers': 'X-Total-Count',
};

export const FAKE_USER = { username: 'facilitator-zq', password: 'a pass-word', email: 'facilitator-zq@example.org' };

export const FAKE_ORGANIZATIONS = [
  { username: 'Equipo-Ejemplo', fullName: 'Equipo Ejemplo', canCreateRepository: true, repositories: { 'es-419': 4 } },
  { username: 'es-419_gl', fullName: 'Latin American Spanish Gateway', canCreateRepository: false, repositories: { 'es-419': 12 } },
];

function decodeBasic(credentials) {
  try {
    return Buffer.from(credentials, 'base64').toString('utf8');
  } catch {
    return null;
  }
}

/** Route every request to `server` (the client's DCS_SERVER) through the fake. */
export async function routeFakeDoor43(context, { server = 'https://git.door43.org', user = FAKE_USER, organizations = FAKE_ORGANIZATIONS } = {}) {
  const valid = new Set();
  const tokens = [];
  let next = 1;
  const answer = (method, url, headers, body) => {
    if (method === 'OPTIONS') return { status: 204 };
    const route = url.pathname.replace(/^\/api\/v1/, '');
    const auth = headers.authorization ?? '';
    const [scheme, credentials] = auth.split(' ');
    const decoded = scheme === 'Basic' ? decodeBasic(credentials ?? '') : null;
    const basicOk = [user.username, user.email].some((login) => decoded === `${login}:${user.password}`);
    const unauthorized = { status: 401, body: { message: 'invalid username, password or token' } };
    if (route === '/user' && scheme === 'Basic') return basicOk ? { status: 200, body: { id: 1, login: user.username, username: user.username, email: user.email } } : unauthorized;
    const tokenRoute = route.match(/^\/users\/([^/]+)\/tokens(?:\/(\d+))?$/);
    if (tokenRoute) {
      if (!basicOk) return unauthorized;
      if (method === 'GET') return { status: 200, body: tokens.map(({ id, name, scopes }) => ({ id, name, scopes })) };
      if (method === 'DELETE') return { status: 204 };
      const { name, scopes = [] } = JSON.parse(body ?? '{}');
      const sha1 = `fake-token-${next}`;
      tokens.push({ id: next++, name, scopes });
      valid.add(sha1);
      return { status: 201, body: { id: next, name, sha1, scopes } };
    }
    if (route.startsWith('/repos/search')) {
      const org = organizations.find((o) => o.username === url.searchParams.get('owner'));
      return { status: 200, headers: { 'X-Total-Count': String(org?.repositories?.[url.searchParams.get('lang')] ?? 0) }, body: { ok: true, data: [] } };
    }
    if (!valid.has(auth.startsWith('token ') ? auth.slice(6) : '')) return unauthorized;
    if (route === '/user') return { status: 200, body: { login: user.username, username: user.username } };
    if (route === '/user/orgs') {
      const page = Number(url.searchParams.get('page') ?? 1);
      return { status: 200, body: page > 1 ? [] : organizations.map((o, i) => ({ id: i + 1, username: o.username, full_name: o.fullName ?? '' })) };
    }
    const permissions = route.match(/^\/users\/([^/]+)\/orgs\/([^/]+)\/permissions$/);
    if (permissions) {
      const org = organizations.find((o) => o.username === permissions[2]);
      if (!org) return { status: 404, body: { message: 'not found' } };
      return { status: 200, body: { can_create_repository: org.canCreateRepository, can_read: true, can_write: org.canCreateRepository, is_admin: false, is_owner: false } };
    }
    return { status: 404, body: { message: 'not served by the tc4-baseline fake Door43' } };
  };
  await context.route(`${server}/**`, async (route) => {
    const request = route.request();
    const out = answer(request.method(), new URL(request.url()), request.headers(), request.postData());
    await route.fulfill({
      status: out.status,
      headers: { ...CORS, 'Content-Type': 'application/json', ...(out.headers ?? {}) },
      body: out.body === undefined ? '' : JSON.stringify(out.body),
    });
  });
}
