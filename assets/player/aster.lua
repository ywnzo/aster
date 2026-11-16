-- Aster's native video overlay. Original implementation; no external OSC.
local mp = require 'mp'
local assdraw = require 'mp.assdraw'
local utils = require 'mp.utils'
local unpack = table.unpack or unpack
local options={confirm='A',back='B',embedded=false}
require('mp.options').read_options(options,'aster')
local state = {menu=nil, cursor=1, first=1, controls=false, timeline=false, focus=1, until_time=0, opacity=0}
local overlay = mp.create_osd_overlay('ass-events')
local W,H = 1280,720
local hits = {}
local render_timer
local accent,white,muted,bg = '9DB9E6','F4F0EE','AEA097','161210'
local focus_color='FFD3A9'
local languages = {eng='English',en='English',ces='Czech',cze='Czech',cs='Czech',deu='German',ger='German',de='German',fra='French',fre='French',fr='French',spa='Spanish',es='Spanish',jpn='Japanese',ja='Japanese',ita='Italian',it='Italian',por='Portuguese',pt='Portuguese',slk='Slovak',slo='Slovak',sk='Slovak',pol='Polish',pl='Polish',kor='Korean',ko='Korean',zho='Chinese',chi='Chinese',zh='Chinese',und='Unknown language'}
local function escape(s) return tostring(s or ''):gsub('\\','＼'):gsub('{','｛'):gsub('}','｝'):gsub('\n',' ') end
local function alpha(n) return string.format('%02X',math.floor(255-(255-(n or 0))*state.opacity)) end
local function show() state.until_time=mp.get_time()+3.5;if render_timer and (not mp.get_property_bool('idle-active',true) or state.up_next) then render_timer:resume() end end
local function time(t)
    t=math.max(0,math.floor(t or 0)); if t>=3600 then return string.format('%d:%02d:%02d',math.floor(t/3600),math.floor(t/60)%60,t%60) end
    return string.format('%d:%02d',math.floor(t/60),t%60)
end
local function language(t) return languages[t.lang] or (t.lang and t.lang:upper()) or 'Unknown language' end
local chapter_list
local function rows(kind)
    local result={}
    if kind=='chapters' then
        local chapters=chapter_list(mp.get_property_number('duration',0))
        local position=mp.get_property_number('time-pos',0)
        for index,chapter in ipairs(chapters) do
            result[#result+1]={id=chapter.time,label=chapter.title,meta=time(chapter.time),selected=position>=chapter.time and (not chapters[index+1] or position<chapters[index+1].time)}
        end
        return result
    end
    if kind=='speed' then
        for _,v in ipairs({0.5,0.75,1,1.25,1.5,2}) do result[#result+1]={id=v,label=string.format('%gx',v),meta=v==1 and 'Normal speed' or '',selected=math.abs((mp.get_property_number('speed',1))-v)<0.01} end
        return result
    end
    local tracks=mp.get_property_native('track-list',{})
    if kind=='sub' then local sid=mp.get_property_native('sid');result[1]={id='no',label='Off',meta='No subtitles',selected=sid==false or sid==nil or sid=='no'} end
    for _,t in ipairs(tracks) do
        if t.type==kind then
            local details={language(t)}
            if t.codec then details[#details+1]=t.codec:upper() end
            if kind=='audio' then
                local channels=t['demux-channel-count']
                if channels then details[#details+1]=channels==2 and 'Stereo' or channels==6 and '5.1 surround' or channels==8 and '7.1 surround' or (channels..' channels') end
                local rate=t['demux-samplerate']; if rate then details[#details+1]=string.format('%g kHz',rate/1000) end
            end
            if t.forced then details[#details+1]='Forced' end
            if t.default then details[#details+1]='Default' end
            if t.external then details[#details+1]='External' end
            result[#result+1]={id=t.id,label=t.title and t.title~='' and t.title or language(t),meta=table.concat(details,'  ·  '),selected=t.selected==true}
        end
    end
    return result
end
local function open(kind)
    state.menu=kind;state.cursor=1;state.first=1;state.controls=false;state.timeline=false
    for i,row in ipairs(rows(kind)) do if row.selected then state.cursor=i end end
    show()
end
local function select_row()
    local row=rows(state.menu)[state.cursor]; if not row then return end
    if state.menu=='chapters' then
        mp.commandv('seek',row.id,'absolute','exact');state.controls=true;state.timeline=true
    else mp.set_property_native(state.menu=='audio' and 'aid' or state.menu=='sub' and 'sid' or 'speed',row.id) end
    state.menu=nil;show()
end
local function stop_player()
    state.up_next=nil;state.menu=nil;state.controls=false;state.timeline=false
    if render_timer then render_timer:kill() end
    if options.embedded then mp.set_property_native('user-data/aster-stop',true);mp.commandv('stop') else mp.commandv('quit') end
end
local function navigate_episode(direction)
    if not state.episode or state.episode_busy or state[direction]==false then return end
    state.episode_busy=true
    mp.set_property_native('user-data/aster-episode-action',direction)
    show()
end
local function button_action(i)
    if i==1 then mp.commandv('cycle','pause')
    elseif i==2 then mp.commandv('seek',-10,'relative')
    elseif i==3 then mp.commandv('seek',10,'relative')
    elseif i==4 then mp.commandv('cycle','mute')
    elseif i==5 then navigate_episode('previous')
    elseif i==6 then navigate_episode('next')
    elseif i==7 then open('audio')
    elseif i==8 then open('sub')
    elseif i==9 then open('speed')
    elseif i==10 then mp.commandv('cycle','fullscreen')
    elseif i==11 then open('chapters') end
    show()
end
local function reset_seek()
    state.seek_direction=nil;state.seek_started=nil;state.seek_step=10
end
local function input(action)
    state.last_input=action;state.input_count=(state.input_count or 0)+1;state.input_history=state.input_history or {};table.insert(state.input_history,action);if #state.input_history>12 then table.remove(state.input_history,1) end
    if action=='seek-release' then reset_seek();show();return end
    local repeated=action=='left-repeat' or action=='right-repeat'
    if action=='left-repeat' then action='left' elseif action=='right-repeat' then action='right' end
    if action~='left' and action~='right' then reset_seek() end
    show()
    if action=='back' then
        if state.up_next then mp.set_property_native('user-data/aster-autoplay-action','cancel') end
        stop_player();return
    end
    if action=='previous' or action=='next' then navigate_episode(action);return end
    if state.up_next then
        if action=='left' then state.up_next.selected=1
        elseif action=='right' then state.up_next.selected=2
        elseif action=='back' or (action=='confirm' and state.up_next.selected==2) then mp.set_property_native('user-data/aster-autoplay-action','cancel');stop_player()
        elseif action=='confirm' and state.up_next.seconds>=0 then mp.set_property_native('user-data/aster-autoplay-action','play');state.up_next=nil end
        return
    end
    if action=='chapters' then
        if #rows('chapters')>0 then open('chapters') else state.notice='No chapters available';state.notice_until=mp.get_time()+3.5 end
        return
    end
    if action=='audio' then open('audio');return end
    if action=='subtitles' then open('sub');return end
    if state.menu then
        if action=='up' then state.cursor=math.max(1,state.cursor-1)
        elseif action=='down' then state.cursor=math.min(#rows(state.menu),state.cursor+1)
        elseif action=='left' and (state.menu=='audio' or state.menu=='sub') then open('audio')
        elseif action=='right' and (state.menu=='audio' or state.menu=='sub') then open('sub')
        elseif action=='confirm' then select_row()
        elseif action=='back' or action=='menu' then state.menu=nil end
        return
    end
    if action=='menu' then state.controls=not state.controls;state.timeline=false;return end
    if action=='up' then state.controls=true;state.timeline=true;return end
    if action=='down' then state.controls=true;state.timeline=false;return end
    if state.controls and state.timeline then
        if action=='left' or action=='right' then
            local duration=mp.get_property_number('duration',0)
            if duration>0 then
                local now=mp.get_time()
                if not repeated or state.seek_direction~=action or not state.seek_started then
                    state.seek_direction=action;state.seek_started=now
                end
                local held=now-state.seek_started
                state.seek_step=held>=3 and 60 or (held>=1.2 and 30 or 10)
                local target=mp.get_property_number('time-pos',0)+(action=='left' and -state.seek_step or state.seek_step)
                mp.commandv('seek',math.max(0,math.min(math.max(0,duration-0.1),target)),'absolute','exact')
            end
        elseif action=='confirm' then mp.commandv('cycle','pause') end
        return
    end
    if action=='left' or action=='right' then state.controls=true end
    if state.controls then
        if action=='left' or action=='right' then
            local order={1,2,3,4,11,5,6,7,8,9,10}
            local index=1
            for i,id in ipairs(order) do if id==state.focus then index=i;break end end
            repeat
                index=(index+(action=='left' and #order-2 or 0))%#order+1
                state.focus=order[index]
            until (state.episode or (state.focus~=5 and state.focus~=6)) and (state.focus~=11 or #rows('chapters')>0)
        elseif action=='confirm' then button_action(state.focus)
        end
    elseif action=='confirm' then mp.commandv('cycle','pause') end
end
local base_sub_margin=mp.get_property_number('sub-margin-y',22)
chapter_list=function(duration)
    local chapters={}
    for index,chapter in ipairs(mp.get_property_native('chapter-list',{})) do
        local stamp=tonumber(chapter.time)
        if stamp and stamp==stamp and stamp>=0 and stamp<duration then
            local title=type(chapter.title)=='string' and chapter.title or ''
            if not title:match('%S') then title='Chapter '..index end
            chapters[#chapters+1]={time=stamp,title=title}
        end
    end
    table.sort(chapters,function(a,b)return a.time<b.time end)
    return chapters
end
local function render()
    local ow,oh=mp.get_osd_size(); if not ow or not oh or oh==0 then return end
    W,H=ow/oh*720,720
    local visible=state.up_next or state.menu or state.controls or mp.get_property_bool('pause') or mp.get_time()<state.until_time
    state.opacity=math.max(0,math.min(1,state.opacity+(visible and 0.18 or -0.13)))
    local position=mp.get_property_number('time-pos',0);local duration=mp.get_property_number('duration',0)
    local chapters=chapter_list(duration)
    local mouse_x,mouse_y=mp.get_mouse_pos()
    local hovering_timeline=mouse_y and math.abs(mouse_y*720/oh-(H-63))<12 and mouse_x*720/oh>=64 and mouse_x*720/oh<=W-70
    local chapter_position=position
    if hovering_timeline then chapter_position=math.max(0,math.min(1,(mouse_x*720/oh-64)/(W-134)))*duration end
    local chapter_title=nil
    local markers={}
    for _,chapter in ipairs(chapters) do
        if chapter.time<=chapter_position then chapter_title=chapter.title end
        if chapter.time>0 then markers[#markers+1]=chapter.time end
    end
    if hovering_timeline then
        for _,chapter in ipairs(chapters) do
            if math.abs(64+(W-134)*chapter.time/math.max(0.001,duration)-mouse_x*720/oh)<6 then chapter_title=chapter.title;break end
        end
    end
    local published={input_history=state.input_history or {},last_input=state.last_input or false,input_count=state.input_count or 0,chapter_count=#chapters,chapter_markers=markers,chapter_title=chapter_title or false,menu=state.menu or 'none',cursor=state.cursor,controls=state.controls,timeline=state.timeline,seek_step=state.seek_step or 10,visible=state.opacity>0,audio=mp.get_property_native('aid'),subtitle=mp.get_property_native('sid'),volume=mp.get_property_number('volume'),episode=state.episode,episode_busy=state.episode_busy,previous=state.previous,next=state.next,focus=state.focus,speed=mp.get_property_number('speed'),up_next=state.up_next and {title=state.up_next.title,seconds=state.up_next.seconds} or false}
    local encoded=utils.format_json(published)
    if encoded~=state.published then state.published=encoded;mp.set_property_native('user-data/aster',published) end
    local preview_space=(state.controls and state.timeline) or hovering_timeline
    local sub_margin=state.opacity>0 and math.max(base_sub_margin,preview_space and 290 or 92) or base_sub_margin
    if state.sub_margin~=sub_margin then mp.set_property_number('sub-margin-y',sub_margin);state.sub_margin=sub_margin end
    if state.opacity==0 then
        hits={};if overlay.data~='' then overlay:remove();overlay.data='' end
        if render_timer then render_timer:kill() end
        return
    end
    local a=assdraw.ass_new();hits={}
    local function rect(x,y,w,h,color,opacity,radius)
        if w<=0 or h<=0 then return end
        a:new_event();a:append(string.format('{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%s&}',color,alpha(opacity)))
        a:draw_start(); if radius then a:round_rect_cw(x,y,x+w,y+h,radius) else a:rect_cw(x,y,x+w,y+h) end;a:draw_stop()
    end
    local function text(x,y,value,size,color,align,width)
        a:new_event();a:append(string.format('{\\an%d\\pos(%g,%g)\\fnDejaVu Sans\\fs%g\\bord0\\shad0\\1c&H%s&\\1a&H%s&}',align or 7,x,y,size,color or white,alpha(0)))
        if width then a:append(string.format('{\\clip(%g,%g,%g,%g)}',x,y,x+width,y+size*1.65)) end
        a:append(escape(value))
    end
    local function hit(x,y,w,h,action) hits[#hits+1]={x=x,y=y,w=w,h=h,action=action} end
    hit(0,0,W,52,function() show() end)
    hit(0,H-82,W,82,function() show() end)
    -- A translucent two-row transport strip, with the video visible underneath.
    for n=0,7 do rect(0,n*7,W,7,'000000',195+n*7) end
    rect(0,H-82,W,82,'181818',150)
    rect(0,H-82,W,1,'FFFFFF',235)
    local function line(x1,y1,x2,y2,color,width)
        local dx,dy=x2-x1,y2-y1;local length=math.sqrt(dx*dx+dy*dy);if length==0 then return end
        local nx,ny=-dy/length*(width or 1.5)/2,dx/length*(width or 1.5)/2
        a:new_event();a:append(string.format('{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%s&}',color,alpha(0)));a:draw_start();a:move_to(x1+nx,y1+ny);a:line_to(x2+nx,y2+ny);a:line_to(x2-nx,y2-ny);a:line_to(x1-nx,y1-ny);a:draw_stop()
    end
    local function triangle(x,y,size,color)
        a:new_event();a:append(string.format('{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%s&}',color,alpha(0)));a:draw_start();a:move_to(x,y-size);a:line_to(x,y+size);a:line_to(x+size*1.4,y);a:draw_stop()
    end
    line(18,25,28,25,white,1.5);line(18,25,23,20,white,1.5);line(18,25,23,30,white,1.5)
    hit(7,8,34,34,function() input('back') end)
    local heading=mp.get_property('media-title','Aster')
    if chapter_title and ((state.controls and state.timeline) or hovering_timeline) then heading=heading..'  ·  '..chapter_title end
    text(48,17,heading,14,white,7,W-90)
    local start_x,end_x=64,W-70;local track_y=H-63
    text(14,track_y-7,time(position),12,white);text(W-14,track_y-7,'−'..time(math.max(0,duration-position)),12,white,9)
    local timeline_focused=state.controls and state.timeline
    if timeline_focused then
        rect(start_x-8,track_y-10,end_x-start_x+16,24,focus_color,205,6)
        text(W/2,track_y-35,string.format('Left / Right: seek %ds   ·   Hold to accelerate   ·   Down: buttons',state.seek_step or 10),12,focus_color,8)
    end
    rect(start_x,track_y,end_x-start_x,3,'FFFFFF',175,1.5)
    local fraction=duration>0 and math.max(0,math.min(1,position/duration)) or 0
    rect(start_x,track_y,(end_x-start_x)*fraction,3,accent,0,1.5)
    rect(start_x+(end_x-start_x)*fraction-4,track_y-2.5,8,8,timeline_focused and focus_color or accent,0,4)
    hit(start_x-5,track_y-11,end_x-start_x+10,25,function(x) state.controls=true;state.timeline=true;if duration>0 then mp.commandv('seek',math.max(0,math.min(100,(x-start_x)/(end_x-start_x)*100)),'absolute-percent','exact') end;show() end)
    for _,chapter in ipairs(chapters) do
        if chapter.time>0 and duration>0 then
            local stamp=chapter.time
            local x=start_x+(end_x-start_x)*stamp/duration
            rect(x-1,track_y-2,2,7,white,35,0.6)
            hit(x-5,track_y-9,10,22,function()state.controls=true;state.timeline=true;reset_seek();mp.commandv('seek',stamp,'absolute','exact');show()end)
        end
    end

    local cy=H-27
    local buttons={{24,28},{62,28},{100,28},{143,28},{W-246,28},{W-208,28},{W-164,30},{W-126,30},{W-88,34},{W-42,30},{W-284,30}}
    local labels={mp.get_property_bool('pause') and 'Play' or 'Pause','Back 10 seconds','Forward 10 seconds',mp.get_property_bool('mute') and 'Unmute' or 'Mute','Previous episode · LB','Next episode · RB','Audio','Subtitles','Playback speed','Fullscreen','Chapters · Y'}
    local mx,my=mp.get_mouse_pos();local osw,osh=mp.get_osd_size();mx=osh and mx*720/osh or -1;my=osh and my*720/osh or -1
    for i,b in ipairs(buttons) do
      if (state.episode or (i~=5 and i~=6)) and (i~=11 or #chapters>0) then
        local cx,width=b[1],b[2];local hovered=mx>=cx-width/2 and mx<=cx+width/2 and my>=cy-16 and my<=cy+16
        local focused=state.controls and not state.timeline and state.focus==i
        if focused then rect(cx-width/2-2,cy-17,width+4,34,focus_color,170,5)
        elseif hovered then rect(cx-width/2-2,cy-17,width+4,34,'FFFFFF',225,5) end
        local disabled=(i==5 and state.previous==false) or (i==6 and state.next==false) or ((i==5 or i==6) and state.episode_busy)
        local color=disabled and muted or (focused and focus_color or white)
        if i==1 then
            if mp.get_property_bool('pause') then triangle(cx-4,cy,6,color)
            else rect(cx-5,cy-6,3,12,color,0,0.5);rect(cx+2,cy-6,3,12,color,0,0.5) end
        elseif i==2 or i==3 then
            local sign=i==2 and -1 or 1
            for n=0,15 do
                local a1=-math.pi/2+sign*(0.30+n/16*5.2);local a2=-math.pi/2+sign*(0.30+(n+1)/16*5.2)
                line(cx+math.cos(a1)*8,cy+math.sin(a1)*8,cx+math.cos(a2)*8,cy+math.sin(a2)*8,color,1.2)
            end
            line(cx+sign*2,cy-8,cx-sign*2,cy-11,color,1.2);line(cx+sign*2,cy-8,cx-sign*2,cy-5,color,1.2)
            text(cx,cy-4,'10',7.5,color,8)
        elseif i==4 then
            rect(cx-7,cy-3,4,6,color,0,0.5)
            a:new_event();a:append(string.format('{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%s&}',color,alpha(0)));a:draw_start();a:move_to(cx-3,cy-3);a:line_to(cx+2,cy-7);a:line_to(cx+2,cy+7);a:line_to(cx-3,cy+3);a:draw_stop()
            if mp.get_property_bool('mute') then line(cx+5,cy-4,cx+11,cy+4,color,1.3);line(cx+5,cy+4,cx+11,cy-4,color,1.3)
            else line(cx+6,cy-4,cx+9,cy,color,1.3);line(cx+9,cy,cx+6,cy+4,color,1.3) end
        elseif i==5 or i==6 then
            local direction=i==5 and -1 or 1
            line(cx+direction*7,cy-6,cx+direction*7,cy+6,color,1.5)
            if i==6 then triangle(cx-5,cy,6,color)
            else
                a:new_event();a:append(string.format('{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%s&}',color,alpha(0)));a:draw_start();a:move_to(cx+5,cy-6);a:line_to(cx+5,cy+6);a:line_to(cx-3,cy);a:draw_stop()
            end
        elseif i==7 then for n=-2,2 do local height=({5,9,14,9,5})[n+3];rect(cx+n*3,cy-height/2,1.5,height,color,0,0.7) end
        elseif i==8 then
            line(cx-9,cy-6,cx+9,cy-6,color,1.2);line(cx-9,cy+6,cx+9,cy+6,color,1.2);line(cx-9,cy-6,cx-9,cy+6,color,1.2);line(cx+9,cy-6,cx+9,cy+6,color,1.2);text(cx,cy-6,'CC',10,color,8)
        elseif i==9 then text(cx,cy-7,string.format('%gx',mp.get_property_number('speed',1)),12,color,8)
        elseif i==11 then
            for row=-1,1 do rect(cx-8,cy+row*5-1,3,2,color,0,0.5);rect(cx-2,cy+row*5-1,11,2,color,0,0.5) end
        elseif i==10 then
            for _,corner in ipairs({{cx-7,cy-6,1,1},{cx+7,cy-6,-1,1},{cx-7,cy+6,1,-1},{cx+7,cy+6,-1,-1}}) do local x,y,dx,dy=unpack(corner);line(x,y,x+dx*5,y,color,1.4);line(x,y,x,y+dy*5,color,1.4) end
        end
        hit(cx-width/2-2,cy-17,width+4,34,function() state.timeline=false;state.focus=i;button_action(i) end)
        if hovered or focused then rect(cx-72,cy-68,144,23,'000000',95,5);text(cx,cy-64,labels[i],11,white,8) end
      end
    end
    if state.notice and mp.get_time()<state.notice_until then text(W/2,H-110,state.notice,15,white,8) end
    if W>=700 then
    local volume=math.max(0,math.min(1,mp.get_property_number('volume',80)/100))
    rect(171,cy-1,68,3,'FFFFFF',165,1.5);rect(171,cy-1,68*volume,3,accent,0,1.5);rect(168+68*volume,cy-3,6,6,white,0,3)
    hit(164,cy-10,82,23,function(x) mp.set_property_number('volume',math.max(0,math.min(100,(x-171)/68*100)));show() end)
    end
    if state.up_next then
        local next=state.up_next;local width=math.min(570,W-80);local height=254;local x,y=(W-width)/2,(H-height)/2
        rect(0,0,W,H,'000000',80);hit(0,0,W,H,function() end);rect(x,y,width,height,bg,4,16)
        text(x+28,y+24,'UP NEXT',13,accent)
        local show_title,episode=next.title:match('^(.-) · (.+)$')
        text(x+28,y+54,show_title or next.title,24,white,7,width-56)
        if episode then text(x+28,y+90,episode,17,muted,7,width-56) end
        text(x+28,y+126,next.seconds<0 and 'Preparing the next episode…' or ('Starting in '..next.seconds..' seconds'),15,muted)
        local bw=(width-68)/2
        for i,label in ipairs({'Play now','Cancel autoplay'}) do
            local bx=x+28+(i-1)*(bw+12);local selected=next.selected==i
            rect(bx,y+166,bw,44,selected and accent or '39312B',0,8);text(bx+bw/2,y+177,label,16,selected and bg or white,8)
            local index=i
            hit(bx,y+166,bw,44,function() if index==1 then if next.seconds>=0 then mp.set_property_native('user-data/aster-autoplay-action','play');state.up_next=nil end else mp.set_property_native('user-data/aster-autoplay-action','cancel');stop_player() end end)
        end
        text(x+28,y+225,options.confirm..': select    '..options.back..': cancel    Left / Right: choose',11,muted)
    end
    if state.menu then
        local items=rows(state.menu);local count=#items
        local width=math.min(530,W-80);local max_rows=6;local row_h=60
        local displayed=math.max(1,math.min(count,max_rows));local height=115+displayed*row_h
        local x,y=(W-width)/2,(H-height)/2-10
        rect(0,0,W,H,'000000',110)
        hit(0,0,W,H,function() state.menu=nil;show() end)
        hit(x,y,width,height,function() end)
        rect(x,y,width,height,bg,8,14)
        text(x+26,y+22,state.menu=='audio' and 'Audio' or state.menu=='sub' and 'Subtitles' or state.menu=='chapters' and 'Chapters' or 'Playback speed',25,white)
        text(x+26,y+58,state.menu=='speed' and 'Choose your pace' or state.menu=='chapters' and 'Choose a chapter' or 'Choose a track',13,muted)
        state.cursor=math.max(1,math.min(math.max(1,count),state.cursor))
        state.first=math.max(1,math.min(state.first,state.cursor))
        if state.cursor>=state.first+max_rows then state.first=state.cursor-max_rows+1 end
        if count==0 then text(x+26,y+108,state.menu=='chapters' and 'No chapters available' or 'No tracks available',18,muted) end
        for i=state.first,math.min(count,state.first+max_rows-1) do
            local row=items[i];local ry=y+88+(i-state.first)*row_h
            if state.cursor==i then rect(x+12,ry-2,width-24,row_h-5,'39312B',0,8) end
            if row.selected then rect(x+23,ry+16,5,20,accent,0,2) end
            text(x+42,ry+6,row.label,18,row.selected and accent or white,7,width-70)
            text(x+42,ry+32,row.meta,11,muted,7,width-70)
            hit(x+12,ry-2,width-24,row_h-5,function() state.cursor=i;select_row() end)
        end
        text(x+26,y+height-19,options.confirm..': select    '..options.back..': stop    D-pad: navigate',11,muted)
        if count>max_rows then text(x+width-25,y+height-19,state.cursor..' / '..count,11,muted,9) end
    end
    if overlay.res_x~=W or overlay.res_y~=H or overlay.data~=a.text then
        overlay.res_x=W;overlay.res_y=H;overlay.data=a.text;overlay.z=1000;overlay:update()
    end
    if mp.get_property_bool('pause') and state.opacity==1 and not state.up_next and render_timer then render_timer:kill() end
end
mp.register_script_message('aster-input',input)
mp.register_script_message('aster-up-next',function(title,seconds)
    local selected=state.up_next and state.up_next.title==title and state.up_next.selected or 1
    state.up_next={title=title,seconds=tonumber(seconds) or -1,selected=selected};state.menu=nil;state.controls=false;show()
end)
mp.register_script_message('aster-clear-next',function()state.up_next=nil;state.menu=nil;state.controls=false;state.timeline=false;reset_seek() end)
mp.register_script_message('aster-episode',function(enabled)
    state.episode=enabled=='yes';state.episode_busy=false;state.previous=true;state.next=true;state.notice=nil
    if not state.episode and (state.focus==5 or state.focus==6) then state.focus=1 end
end)
mp.register_script_message('aster-episode-loading',function() state.episode_busy=true;state.notice='Loading episode…';state.notice_until=mp.get_time()+30;show() end)
mp.register_script_message('aster-episode-unavailable',function(direction) state.episode_busy=false;state[direction]=false;state.notice=direction=='previous' and 'No previous episode' or 'No next episode';state.notice_until=mp.get_time()+3.5;show() end)
mp.register_script_message('aster-episode-error',function() state.episode_busy=false;state.notice='Could not load episode. Try again.';state.notice_until=mp.get_time()+3.5;show() end)
mp.register_script_message('aster-chapters',function(item,serialized)
    local path=mp.get_property('path',''):match('^[^?]*') or ''
    local suffix='/Videos/'..item..'/stream'
    if path:sub(-#suffix)~=suffix or #mp.get_property_native('chapter-list',{})>0 then return end
    local chapters=utils.parse_json(serialized)
    if type(chapters)=='table' and #chapters>0 then mp.set_property_native('chapter-list',chapters);show() end
end)
mp.register_script_message('aster-config',function(confirm,back) options.confirm=confirm;options.back=back end)
mp.register_script_message('aster-menu',function(kind) open(kind=='subtitles' and 'sub' or kind) end)
mp.add_forced_key_binding('mouse_move','aster-show',function() show() end)
local function click_video()
    state.last_input='mouse-click';state.input_count=(state.input_count or 0)+1
    local x,y=mp.get_mouse_pos();local ow,oh=mp.get_osd_size();if not oh or oh==0 then return end;x=x*720/oh;y=y*720/oh
    for i=#hits,1,-1 do local h=hits[i];if x>=h.x and y>=h.y and x<=h.x+h.w and y<=h.y+h.h then h.action(x,y);return end end
    mp.commandv('cycle','pause');show()
end
mp.add_forced_key_binding('MBTN_LEFT','aster-click',click_video)
mp.add_forced_key_binding('MBTN_LEFT_DBL','aster-double-click',click_video)
for key,action in pairs({UP='up',DOWN='down',LEFT='left',RIGHT='right',PGUP='previous',PGDWN='next',ENTER='confirm',ESC='back'}) do
    local mapped=action
    if mapped=='left' or mapped=='right' then
        mp.add_forced_key_binding(key,'aster-'..mapped,function(event)
            if event.event=='up' then input('seek-release')
            elseif event.event=='repeat' then input(mapped..'-repeat')
            elseif event.event=='down' or event.event=='press' then input(mapped) end
        end,{repeatable=true,complex=true})
    else
        mp.add_forced_key_binding(key,'aster-'..mapped,function() input(mapped) end,{repeatable=mapped=='up' or mapped=='down'})
    end
end
mp.add_forced_key_binding('c','aster-open-chapters',function() input('chapters') end)
mp.add_forced_key_binding('a','aster-audio',function() open('audio') end)
mp.add_forced_key_binding('s','aster-subtitles',function() open('sub') end)
mp.add_forced_key_binding('TAB','aster-controls',function() input('menu') end)
mp.add_forced_key_binding('SPACE','aster-pause',function() mp.commandv('cycle','pause');show() end)
mp.observe_property('pause','bool',show)
mp.observe_property('osd-dimensions','native',show)
mp.observe_property('seeking','bool',function(_,seeking)if not seeking then show() end end)
for _,property in ipairs({'volume','mute','speed','aid','sid'}) do mp.observe_property(property,'native',show) end
mp.observe_property('chapter-list','native',show)
mp.register_event('file-loaded',function()state.up_next=nil;state.menu=nil;state.controls=false;state.timeline=false;reset_seek();show() end)
render_timer=mp.add_periodic_timer(1/30,render)
mp.observe_property('idle-active','bool',function(_,idle)
    if idle and not state.up_next then render_timer:kill() else show() end
end)
show()
if mp.get_property_bool('idle-active',true) then render_timer:kill() end
