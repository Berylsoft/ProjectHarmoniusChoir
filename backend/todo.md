# To do

## Api

? for currently skip-able

- [x] user - get info
  - uid
- [x] user - list projects
  - is joined
- [x] user - join project
  - valid question answer
- [x] manager - root - sudo - create manager
  - manager name
- [x] manager - root - list managers
- [x] manager - root - sudo - project manager management
- [x] manager - get info
- [x] manager - list projects
- [x] manager - list project users
  - uid
  - group info after pre submit passed
- [x] user - query project info
  - info
  - status
  - NDA state, include the nda it self after pre submit passed
  - pre submit status detail
  - submit status detail - only rejected
  - include "have_attachment" after nda agreed
- [x] user - upload file
  - check file name
  - /uploads/{project}/{user}/{stage}/{md5}
  - wav/flac/ogg/mp3/m4a/aac
  - start
  - list uploading
  - continue
  - finish
- [x] user - list pending files
  - for the project
  - for current stage
  - include pre-submit file when condition met
- [x] user - delete uploading/pending file
- [?] manager - root - list files #TODO #LOW_PRIORITY
  - paging
- [x] user - pre submit
  - name
  - allow using pre submit skip password
- [x] manager - pre submit info
  - name
- [x] manager - file download/preview
- [x] manager - pre submit review
- [x] user - agree NDA
- [x] user - download attachment
  - after pre submit pass and NDA agreed, download attachment(if exists)
- [x] user - submit
  - include of pre-submit file if user in choir group
- [x] manager - submit info
- [x] manager - submit review
  - files check box
- [x] manager - upload file
  - check file name
  - /uploads/{project}/{user}/{stage}/{md5}
  - limit file type to WAV
  - start
  - list uploading
  - continue
  - finish
- [x] manager - list pending files
  - for the project and user
  - for current stage
- [x] manager - delete uploading/pending file
- [x] manager - submit master
- [x] manager - master info
- [x] manager - sudo - bundle job
  - /uploads/{project}/mixed/{job_id}.tar
  - tar: {project_user_id}\_{user_name}_{file_id}_{file_name}
    - and sanitize
  - bundle select project user's master files
  - mark mixed after job finish
  - list jobs

Other to do:

- [x] project end_time
- [x] manager name
  - in list_project_users, for review/master
- [x] allow multiple files in master
  - change submit file limit to 100
  - check lower bound of 1 file for submit and master
- [x] disallow (pre)submit while have uploading files
- [x] add choir harmony group
- [ ] user file capacity manage
- [ ] notification #IMPORTANT
- [ ] limit (pre)submit comment length to 200
- [ ] database indexes #IMPORTANT
- [ ] token renew
- [ ] use Box\<str> directly for those FromRow
- [ ] log every event
- [ ] review error paths
- [ ] more tests
- [ ] user delete

## UI

- [ ] manage - notify change password after first login
- [ ] manage - project user list - show "hidden by default" non-passed puname
- [-] manage - project user list - filter by status
  - backend impl #TODO
