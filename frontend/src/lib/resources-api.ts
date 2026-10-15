import { env } from '$env/dynamic/public'
import { PUBLIC_RESOURCE_AVAILABILITY_URL } from '$env/static/public'
import type { ResourceAvailability } from '$lib/resources'

export async function fetchResourceAvailability(
  langCode: string,
  apiRootUrl: string = <string>env.PUBLIC_BACKEND_API_URL,
  resourceAvailabilityUrl: string = <string>PUBLIC_RESOURCE_AVAILABILITY_URL
): Promise<ResourceAvailability> {
  const response = await fetch(`${apiRootUrl}${resourceAvailabilityUrl}${langCode}`)
  if (!response.ok) throw new Error(response.statusText)
  return response.json()
}
