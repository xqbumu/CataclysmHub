WITH owners AS (
  SELECT 'session' AS owner_kind,id AS owner_id,'game.launch' AS operation,state
    FROM session WHERE state IN ('starting','running','interrupted_unknown')
  UNION ALL SELECT 'scan',id,operation,state FROM task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'backup',id,operation,state FROM backup_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'restore',id,'backup.restore_new',state FROM restore_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'inspection',id,'backup.inspect_restored',state FROM inspection_task WHERE state IN ('running','cancelling')
  UNION ALL SELECT 'installation_copy',id,'installation.copy',state FROM installation_copy_task WHERE state IN ('running','cancelling')
), directories AS (
  SELECT 'session' AS owner_kind,s.id AS owner_id,i.working_dir AS path,'write' AS access
    FROM session s JOIN installation i ON i.id=s.installation_id
  UNION ALL SELECT 'session',id,instance_data_root,'write' FROM session WHERE instance_data_root IS NOT NULL
  UNION ALL SELECT 'scan',t.id,w.save_dir,'read' FROM task t JOIN workspace w ON w.id=t.workspace_id
  UNION ALL SELECT 'backup',t.id,w.backup_dir,CASE WHEN t.operation='backup.create' THEN 'write' ELSE 'read' END
    FROM backup_task t JOIN workspace w ON w.id=t.workspace_id
  UNION ALL SELECT 'backup',t.id,COALESCE(t.source_root,w.save_dir),'read' FROM backup_task t JOIN workspace w ON w.id=t.workspace_id WHERE t.operation='backup.create'
  UNION ALL SELECT 'restore',t.id,w.backup_dir,'read' FROM restore_task t JOIN workspace w ON w.id=t.workspace_id
  UNION ALL SELECT 'restore',id,destination,'write' FROM restore_task
  UNION ALL SELECT 'inspection',t.id,r.destination,'read' FROM inspection_task t JOIN restore_task r ON r.id=t.restore_task_id
  UNION ALL SELECT 'inspection',t.id,w.backup_dir,'read' FROM inspection_task t JOIN restore_task r ON r.id=t.restore_task_id JOIN workspace w ON w.id=r.workspace_id
  UNION ALL SELECT 'installation_copy',id,source_root,'read' FROM installation_copy_task
  UNION ALL SELECT 'installation_copy',id,destination,'write' FROM installation_copy_task
)
SELECT owner_kind,owner_id,operation,state,'global' AS scope,'exclusive' AS access,NULL AS path,
  CASE WHEN owner_kind='session' THEN 'unknown_game_writes' ELSE 'task_serialization' END AS reason FROM owners
UNION ALL
SELECT o.owner_kind,o.owner_id,o.operation,o.state,'directory',d.access,d.path,'declared_directory'
  FROM owners o JOIN directories d ON o.owner_kind=d.owner_kind AND o.owner_id=d.owner_id
ORDER BY owner_kind,owner_id,scope,path
