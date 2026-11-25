-- KEYS[1] job hash, KEYS[2] lease key, KEYS[3] stream
-- ARGV: task_id, stream_id, owner, lease_ttl_ms, group
-- Returns {kind, payload, attempt} or false when the delivery is a stale duplicate (dropped).
local st = redis.call('HGET', KEYS[1], 'st')
local function drop()
  redis.call('XACK', KEYS[3], ARGV[5], ARGV[2])
  redis.call('XDEL', KEYS[3], ARGV[2])
  return false
end
if not st or st == 'done' or st == 'failed' then return drop() end
local cur = redis.call('GET', KEYS[2])
if cur and cur ~= ARGV[3] then return drop() end
if st == 'running' and redis.call('HGET', KEYS[1], 'stream_id') ~= ARGV[2] then return drop() end
local attempt = redis.call('HINCRBY', KEYS[1], 'attempt', 1)
redis.call('HSET', KEYS[1], 'st', 'running', 'owner', ARGV[3], 'stream_id', ARGV[2], 'retrying', 0)
redis.call('SET', KEYS[2], ARGV[3], 'PX', ARGV[4])
return {redis.call('HGET', KEYS[1], 'kind'), redis.call('HGET', KEYS[1], 'payload'), attempt, redis.call('HGET', KEYS[1], 'rid') or ''}
