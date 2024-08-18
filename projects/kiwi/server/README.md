* Have a separate service responsible for creating the machines
* Have the runner just choose any of the open machines to run one
* This allows having a constant available runner up, e.g. hetzner that can be chosen -- different from the ephemeral runners
* Creates a question on how to get the data to where it needs to go -- less of a problem if there are no inputs

* Run everything inside docker containers? Have that be the service definition?