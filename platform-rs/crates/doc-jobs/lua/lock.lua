-- Lock with lease and fencing token. KEYS[1] lock key, KEYS[2] fence counter. ARGV: mode, token, ttl_ms
-- acquire: {1, fence} when granted (or already held by token), {0, 0} when held by another
-- renew: 1 / 0; release: 1 / 0; holds: 1 / 0
local cur = redis.call('GET', KEYS[1])
if ARGV[1] == 'acquire' then
  if not cur then
    local fence = redis.call('INCR', KEYS[2])
    redis.call('SET', KEYS[1], ARGV[2] .. ':' .. fence, 'PX', ARGV[3])
    return {1, fence}
  end
  local t, f = string.match(cur, '^(.*):(%d+)$')
  if t == ARGV[2] then
    redis.call('PEXPIRE', KEYS[1], ARGV[3])
    return {1, tonumber(f)}
  end
  return {0, 0}
end
local t = cur and string.match(cur, '^(.*):%d+$')
if t ~= ARGV[2] then return 0 end
if ARGV[1] == 'renew' then
  redis.call('PEXPIRE', KEYS[1], ARGV[3])
elseif ARGV[1] == 'release' then
  redis.call('DEL', KEYS[1])
end
return 1
