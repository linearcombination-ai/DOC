-- park for a delay (mode "later") or give back at once without counting the attempt (mode "release")
-- KEYS: job hash, lease key, in-flight key, stream, delayed zset
-- ARGV: owner, mode, delay_ms, last_failure_json, stream_id, task_id, group, in_flight_ttl_ms
if redis.call('HGET', KEYS[1], 'owner') ~= ARGV[1] or redis.call('HGET', KEYS[1], 'st') ~= 'running' then return 0 end
redis.call('DEL', KEYS[2])
redis.call('XACK', KEYS[4], ARGV[7], ARGV[5])
redis.call('XDEL', KEYS[4], ARGV[5])
redis.call('HSET', KEYS[1], 'owner', '')
redis.call('PEXPIRE', KEYS[3], ARGV[8])
if ARGV[2] == 'later' then
  local t = redis.call('TIME')
  local now = t[1] * 1000 + math.floor(t[2] / 1000)
  redis.call('HSET', KEYS[1], 'st', 'retrying', 'retrying', 1, 'failure', ARGV[4])
  redis.call('ZADD', KEYS[5], now + tonumber(ARGV[3]), ARGV[6])
else
  redis.call('HINCRBY', KEYS[1], 'attempt', -1)
  redis.call('HSET', KEYS[1], 'st', 'queued')
  redis.call('XADD', KEYS[4], '*', 'id', ARGV[6])
end
return 1
