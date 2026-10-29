WITH owners AS (
  SELECT 'session' AS owner_kind,id AS owner_id,'game.launch' AS operation,state
    FROM session WHERE state IN ('starting','running','interrupted_unknown')
  UNION ALL SELECT 'scan',id,operation,state FROM task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'backup',id,operation,state FROM backup_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'restore',id,'backup.restore_new',state FROM restore_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'inspection',id,'backup.inspect_restored',state FROM inspection_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'installation_copy',id,'installation.copy',state FROM installation_copy_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'content_install',id,'content.install',state FROM content_install_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'config_change',id,'config.apply',state FROM config_change WHERE state IN ('applying','recovery_required')
), directories AS (
  SELECT 'session' AS owner_kind,s.id AS owner_id,i.working_dir AS path,'write' AS access,'installation' AS role
    FROM session s JOIN installation i ON i.id=s.installation_id
  UNION ALL SELECT 'session',id,instance_data_root,'write','other' FROM session WHERE instance_data_root IS NOT NULL
  UNION ALL SELECT 'scan',t.id,w.save_dir,'read','other' FROM task t JOIN workspace w ON w.id=t.workspace_id
  UNION ALL SELECT 'backup',t.id,w.backup_dir,CASE WHEN t.operation='backup.create' THEN 'write' ELSE 'read' END,'other'
    FROM backup_task t JOIN workspace w ON w.id=t.workspace_id
  UNION ALL SELECT 'backup',t.id,COALESCE(t.source_root,w.save_dir),'read','other' FROM backup_task t JOIN workspace w ON w.id=t.workspace_id WHERE t.operation='backup.create'
  UNION ALL SELECT 'restore',t.id,w.backup_dir,'read','other' FROM restore_task t JOIN workspace w ON w.id=t.workspace_id
  UNION ALL SELECT 'restore',id,destination,'write','other' FROM restore_task
  UNION ALL SELECT 'inspection',t.id,r.destination,'read','other' FROM inspection_task t JOIN restore_task r ON r.id=t.restore_task_id
  UNION ALL SELECT 'inspection',t.id,w.backup_dir,'read','other' FROM inspection_task t JOIN restore_task r ON r.id=t.restore_task_id JOIN workspace w ON w.id=r.workspace_id
  UNION ALL SELECT 'config_change',id,data_root,'write','other' FROM config_change
  UNION ALL SELECT 'content_install',id,source_root,'read','other' FROM content_install_task
  UNION ALL SELECT 'content_install',id,destination,'write','other' FROM content_install_task
  UNION ALL SELECT 'installation_copy',id,source_root,'read','other' FROM installation_copy_task
  UNION ALL SELECT 'installation_copy',id,destination,'write','other' FROM installation_copy_task
)
SELECT owner_kind,owner_id,operation,state,'global' AS scope,'exclusive' AS access,NULL AS path,
  CASE WHEN owner_kind='session' THEN 'unknown_game_writes' ELSE 'task_serialization' END AS reason FROM owners
UNION ALL
SELECT o.owner_kind,o.owner_id,o.operation,o.state,'directory',d.access,d.path,CASE WHEN d.role='installation' THEN 'installation_working_directory' ELSE 'declared_directory' END
  FROM owners o JOIN directories d ON o.owner_kind=d.owner_kind AND o.owner_id=d.owner_id
ORDER BY owner_kind,owner_id,scope,path
