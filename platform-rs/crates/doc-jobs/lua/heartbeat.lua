-- KEYS[1] lease key, KEYS[2] job hash; ARGV: owner, lease_ttl_ms
-- 1 = still ours (lease extended, or re-taken after it lapsed with nobody else claiming), 0 = lost
local cur = redis.call('GET', KEYS[1])
if cur == ARGV[1] then
  redis.call('PEXPIRE', KEYS[1], ARGV[2])
  return 1
end
if not cur and redis.call('HGET', KEYS[2], 'owner') == ARGV[1] and redis.call('HGET', KEYS[2], 'st') == 'running' then
  redis.call('SET', KEYS[1], ARGV[1], 'PX', ARGV[2])
  return 1
end
return 0
