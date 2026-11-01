# Open: older items

[Up: table of contents](TOC.md)

* First run of the new trim on the 9 images in `JB core img batch` (old vs new exe, scratch): 8 give the
  same edge tags and colours. IMG_2619_indigo loses its mask (25.6% -> 0%): the crop (327x919 of
  672x1000) is found empty by the model, so BGC/FGC drop out; EIX stays 0000. On 99218793_det1 a soft
  gradient backdrop falls outside the sampled colour range, so the crop stays wide (836x965 of 1000x1000).
  **Note:** the trim may be under active refactor in another session (plan
  `~/.claude/plans/read-these-plans-in-elegant-teacup.md`) — confirm there before acting on this.
* **Removed 2026-10-29 (user):** the ImageNet counter-balance (`counter_balance_for_birefnet`, commit
  7e5ed75). It squeezed the model input and gave empty masks on the 104051881/104051882 boot photos
  (0% with it, 7-10% without). The model now sees the colour-balanced crop directly.
