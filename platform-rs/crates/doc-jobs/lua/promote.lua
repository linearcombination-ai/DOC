-- KEYS: delayed zset, stream; ARGV: max, job_key_prefix
local t = redis.call('TIME')
local now = t[1] * 1000 + math.floor(t[2] / 1000)
local due = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', now, 'LIMIT', 0, ARGV[1])
local n = 0
for _, id in ipairs(due) do
  if redis.call('ZREM', KEYS[1], id) == 1 then
    redis.call('HSET', ARGV[2] .. id, 'st', 'queued', 'retrying', 0)
    redis.call('XADD', KEYS[2], '*', 'id', id)
    n = n + 1
  end
end
return n
