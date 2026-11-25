-- KEYS[1] job hash, KEYS[2] history list; ARGV: owner, progress, history_ttl_s
-- The history is an audit trail (every progress string in order, across attempts), not part of the API.
if redis.call('HGET', KEYS[1], 'owner') == ARGV[1] and redis.call('HGET', KEYS[1], 'st') == 'running' then
  redis.call('HSET', KEYS[1], 'progress', ARGV[2])
  redis.call('RPUSH', KEYS[2], ARGV[2])
  redis.call('EXPIRE', KEYS[2], ARGV[3])
  return 1
end
return 0
